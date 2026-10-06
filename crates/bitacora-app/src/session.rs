//! `GraphSession`: runs a graph's [`bitacora_runtime::Session`] on a background thread.
//!
//! The runtime composes the index, the single-writer command queue, the file watcher and the
//! MCP endpoint (sync stays off unless configured, ADR-024). This wrapper owns the session on
//! one std thread, reports [`SessionEvent`]s through an `async_channel` that a GPUI view drains
//! with [`crate::events::EventPump`] or a spawned task, and performs the ordered shutdown
//! (`Session::shutdown`), either on [`GraphSession::close`] / drop or on request with
//! [`GraphSession::shutdown_with_report`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use bitacora_config::EffectiveConfig;
use bitacora_core::queue::{CommandQueue, QueueEvent};
use bitacora_index::{IndexEvent, ReconcileStats};
use bitacora_mcp::McpConfig;
use bitacora_runtime::{
    DEFAULT_SHUTDOWN_BUDGET, McpOptions, RuntimeConfig, RuntimeError, RuntimeEvent, Session,
    ShutdownReport,
};

use crate::data::{GraphHandle, ViewSettings};

/// Time the app gives the ordered shutdown when the user quits.
pub const SHUTDOWN_BUDGET: Duration = Duration::from_secs(8);

/// How often the session thread looks for control messages while it forwards events.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// What the session thread reports.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// The index is open and reconciled; the read API of the index is available.
    Reader(GraphHandle),
    /// Handles to the live session (the command queue, the effective config, the MCP endpoint).
    Live(SessionLink),
    /// The index changed after the startup reconcile finished (file edits, sync, rename).
    Index(IndexEvent),
    /// Something the user should know about (write failures, conflicts...).
    Notice(SessionNotice),
    /// The reconcile finished; the graph is browsable and kept up to date.
    Ready(SessionSummary),
    /// Opening or reconciling failed; the message is user-presentable.
    Failed(String),
}

/// Handles to the running session, usable from the UI thread.
#[derive(Debug, Clone)]
pub struct SessionLink {
    /// The single-writer command queue (every write goes through it).
    pub queue: CommandQueue,
    /// The effective configuration the session started with.
    pub config: Arc<EffectiveConfig>,
    /// The MCP endpoint, when the server runs.
    pub mcp_endpoint: Option<String>,
}

/// A runtime condition worth a notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionNotice {
    /// Pages could not be written; they stay dirty and are retried.
    WriteFailed(usize),
    /// Pages were not written because their file changed on disk.
    Conflict(usize),
    /// The watcher fell back to polling or lost events.
    WatcherDegraded,
    /// `logseq/config.edn` changed on disk (settings are read at open).
    ConfigChanged,
    /// A file could not be indexed.
    IndexError {
        /// Graph-relative path.
        path: String,
        /// Cause.
        message: String,
    },
    /// The MCP endpoint could not start (the session runs without it).
    McpUnavailable(String),
}

/// Result of the startup reconcile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionSummary {
    /// Files found.
    pub scanned: usize,
    /// Files parsed and written.
    pub parsed: usize,
    /// Files that could not be indexed.
    pub errors: usize,
    /// The cold-build path was used.
    pub cold_build: bool,
    /// Wall time in milliseconds.
    pub elapsed_ms: u64,
}

impl From<&ReconcileStats> for SessionSummary {
    fn from(s: &ReconcileStats) -> Self {
        Self {
            scanned: s.scanned,
            parsed: s.parsed,
            errors: s.errors.len(),
            cold_build: s.cold_build,
            elapsed_ms: u64::try_from(s.elapsed.as_millis()).unwrap_or(u64::MAX),
        }
    }
}

/// Where the session keeps its index and whether it serves MCP.
#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    /// Explicit data directory (tests, portable mode); `None` uses the platform data dir.
    pub data_dir: Option<PathBuf>,
    /// Token file of the MCP endpoint; `None` leaves the MCP server off.
    pub mcp_token_path: Option<PathBuf>,
    /// Global config file; `None` uses the platform default.
    pub global_config: Option<PathBuf>,
}

enum Control {
    Shutdown(Duration, mpsc::Sender<ShutdownReport>),
}

/// A running graph session.
#[derive(Debug)]
pub struct GraphSession {
    root: PathBuf,
    control: Option<mpsc::Sender<Control>>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Control")
    }
}

impl GraphSession {
    /// Starts the session thread and returns the receiver of its events.
    pub fn start(
        root: PathBuf,
        options: SessionOptions,
    ) -> std::io::Result<(Self, Receiver<SessionEvent>)> {
        let (tx, rx) = async_channel::unbounded();
        let (control_tx, control_rx) = mpsc::channel();
        let thread_root = root.clone();
        let thread = std::thread::Builder::new()
            .name("bitacora-session".into())
            .spawn(move || run(&thread_root, &options, &tx, &control_rx))?;
        Ok((
            Self {
                root,
                control: Some(control_tx),
                thread: Some(thread),
            },
            rx,
        ))
    }

    /// The graph folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ordered shutdown within `budget`; blocks until it finished. `None` when the session
    /// thread was already gone (a failed open has nothing to flush).
    pub fn shutdown_with_report(mut self, budget: Duration) -> Option<ShutdownReport> {
        let control = self.control.take()?;
        let (reply_tx, reply_rx) = mpsc::channel();
        let report = control
            .send(Control::Shutdown(budget, reply_tx))
            .ok()
            .and_then(|()| reply_rx.recv().ok());
        if let Some(thread) = self.thread.take() {
            // A panicked session thread has nothing left to clean up.
            let _ = thread.join();
        }
        report
    }

    /// Stops the session (default budget) and waits until it is closed.
    pub fn close(self) {
        let _ = self.shutdown_with_report(DEFAULT_SHUTDOWN_BUDGET);
    }
}

impl Drop for GraphSession {
    fn drop(&mut self) {
        // Closing the channel is the stop signal; the thread shuts down on its own.
        self.control.take();
    }
}

fn runtime_config(root: &Path, options: &SessionOptions, with_mcp: bool) -> RuntimeConfig {
    let mut cfg = RuntimeConfig::new(root);
    cfg.data_dir.clone_from(&options.data_dir);
    if options.global_config.is_some() {
        cfg.global_config.clone_from(&options.global_config);
    }
    if with_mcp && let Some(token_path) = &options.mcp_token_path {
        cfg.mcp = Some(McpOptions {
            config: McpConfig::default(),
            token_path: token_path.clone(),
        });
    }
    cfg
}

/// Opens the session; a failing MCP endpoint (port in use, unwritable token file) must not
/// keep the graph closed, so it is retried without it and reported.
fn open(
    root: &Path,
    options: &SessionOptions,
    tx: &Sender<SessionEvent>,
) -> Result<Session, RuntimeError> {
    match Session::open(runtime_config(root, options, true)) {
        Err(RuntimeError::Mcp(e)) => {
            tracing::warn!("MCP endpoint unavailable: {e}");
            let _ = tx.send_blocking(SessionEvent::Notice(SessionNotice::McpUnavailable(
                e.to_string(),
            )));
            Session::open(runtime_config(root, options, false))
        }
        other => other,
    }
}

fn run(
    root: &Path,
    options: &SessionOptions,
    tx: &Sender<SessionEvent>,
    control: &mpsc::Receiver<Control>,
) {
    let session = match open(root, options, tx) {
        Ok(session) => session,
        Err(e) => {
            tracing::error!(graph = %root.display(), "graph session failed: {e}");
            let _ = tx.send_blocking(SessionEvent::Failed(e.to_string()));
            return;
        }
    };
    // The receiver is gone when the view was closed: nothing left to report to.
    let send = |event| {
        let _ = tx.send_blocking(event);
    };
    let runtime_events = session.subscribe();
    let index_events = session.index_events();
    send(SessionEvent::Reader(GraphHandle {
        reader: session.read_api(),
        root: session.root().to_path_buf(),
        settings: Arc::new(ViewSettings::from_config(session.config())),
    }));
    send(SessionEvent::Live(SessionLink {
        queue: session.queue().clone(),
        config: Arc::new(session.config().clone()),
        mcp_endpoint: session.mcp_endpoint(),
    }));
    send(SessionEvent::Ready(
        session
            .open_stats()
            .map(SessionSummary::from)
            .unwrap_or_default(),
    ));
    if session.watcher_is_polling() == Some(true) {
        send(SessionEvent::Notice(SessionNotice::WatcherDegraded));
    }

    let budget = loop {
        match control.try_recv() {
            Ok(Control::Shutdown(budget, reply)) => {
                let report = session.shutdown(budget);
                let _ = reply.send(report);
                return;
            }
            Err(TryRecvError::Disconnected) => break DEFAULT_SHUTDOWN_BUDGET,
            Err(TryRecvError::Empty) => {}
        }
        if let Some(rx) = &index_events {
            while let Ok(event) = rx.try_recv() {
                send(SessionEvent::Index(event));
            }
        }
        match runtime_events.recv_timeout(POLL_INTERVAL) {
            Ok(event) => {
                if let Some(notice) = notice_for(&event) {
                    send(SessionEvent::Notice(notice));
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
    };
    let report = session.shutdown(budget);
    if !report.is_clean() {
        tracing::warn!(?report, "graph session closed with unwritten files");
    }
}

/// Maps a runtime event to a user notice, when it deserves one.
fn notice_for(event: &RuntimeEvent) -> Option<SessionNotice> {
    match event {
        RuntimeEvent::Queue(QueueEvent::WriteFailed { failed, .. }) => {
            Some(SessionNotice::WriteFailed(failed.len()))
        }
        RuntimeEvent::Queue(QueueEvent::Conflict(keys)) => {
            Some(SessionNotice::Conflict(keys.len()))
        }
        RuntimeEvent::Watch(_) => Some(SessionNotice::WatcherDegraded),
        RuntimeEvent::ConfigChanged => Some(SessionNotice::ConfigChanged),
        RuntimeEvent::IndexError { path, message } => Some(SessionNotice::IndexError {
            path: path.clone(),
            message: message.clone(),
        }),
        _ => None,
    }
}

/// Picks the page shown right after opening a graph: `requested` (a file path relative to the
/// graph, or a page file name without extension), else the newest journal, else the first page.
/// Returns the file and a display title.
pub fn initial_page(root: &Path, requested: Option<&str>) -> Option<(PathBuf, String)> {
    let md_files = |dir: &str| -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        files.sort();
        files
    };
    let titled = |file: PathBuf| {
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_journal = file
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "journals");
        let title = if is_journal {
            stem.replace('_', "-")
        } else {
            stem.replace("___", "/")
        };
        (file, title)
    };
    if let Some(req) = requested {
        let direct = root.join(req);
        if direct.is_file() {
            return Some(titled(direct));
        }
        for dir in ["pages", "journals"] {
            let candidate = root.join(dir).join(format!("{req}.md"));
            if candidate.is_file() {
                return Some(titled(candidate));
            }
        }
    }
    md_files("journals")
        .pop()
        .or_else(|| md_files("pages").into_iter().next())
        .map(titled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("pages")).expect("pages");
        std::fs::create_dir_all(root.join("journals")).expect("journals");
        std::fs::create_dir_all(root.join("logseq")).expect("logseq");
        std::fs::write(root.join("logseq/config.edn"), "{}").expect("config");
        std::fs::write(root.join("pages/Alpha.md"), "- hello [[Beta]]\n").expect("alpha");
        std::fs::write(root.join("pages/Beta.md"), "- b\n").expect("beta");
        std::fs::write(root.join("journals/2024_01_01.md"), "- old\n").expect("j1");
        std::fs::write(root.join("journals/2024_02_01.md"), "- new\n").expect("j2");
        tmp
    }

    /// Events up to and including `Ready` / `Failed`.
    fn until_ready(rx: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
        let mut out = Vec::new();
        while let Ok(event) = rx.recv_blocking() {
            let end = matches!(event, SessionEvent::Ready(_) | SessionEvent::Failed(_));
            out.push(event);
            if end {
                break;
            }
        }
        out
    }

    fn options(data: &tempfile::TempDir) -> SessionOptions {
        SessionOptions {
            data_dir: Some(data.path().to_path_buf()),
            mcp_token_path: None,
            global_config: Some(data.path().join("no-global-config.edn")),
        }
    }

    #[test]
    fn initial_page_prefers_request_then_latest_journal() {
        let g = graph();
        let (file, title) = initial_page(g.path(), None).expect("page");
        assert!(file.ends_with("journals/2024_02_01.md"));
        assert_eq!(title, "2024-02-01");
        let (file, title) = initial_page(g.path(), Some("Beta")).expect("page");
        assert!(file.ends_with("pages/Beta.md"));
        assert_eq!(title, "Beta");
        let (file, _) = initial_page(g.path(), Some("pages/Alpha.md")).expect("page");
        assert!(file.ends_with("pages/Alpha.md"));
    }

    #[test]
    fn session_indexes_the_graph_and_shuts_down_in_order() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("start");
        let events = until_ready(&rx);
        assert!(
            matches!(events.first(), Some(SessionEvent::Reader(_))),
            "{events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(e, SessionEvent::Live(_))),
            "{events:?}"
        );
        let Some(SessionEvent::Ready(summary)) = events.last() else {
            panic!("expected Ready, got {events:?}");
        };
        assert_eq!(summary.scanned, 5);
        assert_eq!(summary.parsed, 5);
        assert!(summary.cold_build);
        let report = session
            .shutdown_with_report(Duration::from_secs(10))
            .expect("report");
        assert!(report.is_clean(), "{report:?}");

        // Reopening the same graph is a warm start: nothing to parse.
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("restart");
        let events = until_ready(&rx);
        let Some(SessionEvent::Ready(summary)) = events.last() else {
            panic!("expected Ready, got {events:?}");
        };
        assert_eq!(summary.parsed, 0);
        session.close();
    }

    #[test]
    fn external_edits_reach_the_views_as_index_events() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("start");
        until_ready(&rx);
        std::fs::write(g.path().join("pages/Beta.md"), "- changed\n").expect("edit");
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut seen = false;
        while std::time::Instant::now() < deadline && !seen {
            if let Ok(event) = rx.try_recv() {
                seen = matches!(
                    event,
                    SessionEvent::Index(IndexEvent::FileReplaced { ref path, .. })
                        if path == "pages/Beta.md"
                );
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        assert!(seen, "no index event for the external edit");
        session.close();
    }

    #[test]
    fn mcp_endpoint_starts_when_a_token_file_is_given() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let mut opts = options(&data);
        opts.mcp_token_path = Some(data.path().join("tokens.json"));
        let (session, rx) = GraphSession::start(g.path().to_path_buf(), opts).expect("start");
        let events = until_ready(&rx);
        let endpoint = events.iter().find_map(|e| match e {
            SessionEvent::Live(link) => Some(link.mcp_endpoint.clone()),
            _ => None,
        });
        // The default port may be taken on the host; then the session still opens and says so.
        let unavailable = events
            .iter()
            .any(|e| matches!(e, SessionEvent::Notice(SessionNotice::McpUnavailable(_))));
        assert!(
            matches!(endpoint, Some(Some(_))) || unavailable,
            "{events:?}"
        );
        assert!(matches!(events.last(), Some(SessionEvent::Ready(_))));
        session.close();
    }

    #[test]
    fn missing_graph_folder_reports_failure() {
        let data = tempfile::tempdir().expect("data");
        let (session, rx) =
            GraphSession::start(data.path().join("nope"), options(&data)).expect("start");
        let events = until_ready(&rx);
        assert!(
            matches!(events.last(), Some(SessionEvent::Failed(_))),
            "{events:?}"
        );
        session.close();
    }
}
