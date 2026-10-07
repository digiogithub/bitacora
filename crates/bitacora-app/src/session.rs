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
use bitacora_core::editor::ConflictNotice;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, QueueEvent};
use bitacora_index::{IndexEvent, ReconcileStats};
use bitacora_runtime::{
    DEFAULT_SHUTDOWN_BUDGET, McpOptions, RuntimeConfig, RuntimeError, RuntimeEvent, Session,
    ShutdownReport, SyncOptions, SyncStatusView,
};
use bitacora_sync::CliConfig;
use bitacora_sync::credentials::CredentialProvider;

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
    /// The sync engine published a new status (BIT-US-0047). Only sent while sync runs.
    Sync(Box<SyncStatusView>),
    /// A page with unsaved edits changed on disk in a way that cannot be merged: ours is kept
    /// and writes to the page are stopped until the user decides (BIT-US-0070).
    DiskConflict(Arc<ConflictNotice>),
    /// A page left the conflicted state (reloaded from disk or resolved elsewhere).
    DiskConflictCleared(PageKey),
    /// The block being edited changed on disk: the editor offers keep mine / take disk
    /// (BIT-T-0344).
    EditingConflict(Box<bitacora_core::editor::EditingConflict>),
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
    /// The block being edited, as the MCP server's write gate (`BLOCK_BUSY`).
    pub gate: Arc<crate::editing::EditingGate>,
    /// Index-backed lookups for page renames.
    pub lookup: Arc<bitacora_runtime::IndexRefLookup>,
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
    /// Sync is configured but could not start (the session runs without it).
    SyncUnavailable(String),
    /// What startup recovery did or found (stale lock, interrupted merge, ...), BIT-US-0047.
    SyncRecovery(String),
    /// The block being edited changed on disk (BIT-T-0344).
    EditingBlockChanged,
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

/// Sync of one graph, as the app configures it (BIT-US-0043).
#[derive(Clone)]
pub struct SyncSetup {
    /// Branch to sync.
    pub branch: String,
    /// `Bitacora-Device` trailer value.
    pub device: String,
    /// System-backend tunables (askpass helper and environment).
    pub cli: Option<CliConfig>,
    /// Credential provider for the built-in backend.
    pub credentials: Option<Arc<dyn CredentialProvider>>,
    /// Commit and fetch timings, applied to the engine (BIT-T-0295).
    pub timing: crate::sync_prefs::SyncPrefs,
}

impl std::fmt::Debug for SyncSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncSetup")
            .field("branch", &self.branch)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

/// Where the session keeps its index and whether it serves MCP.
#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    /// Sync; `None` leaves it off.
    pub sync: Option<SyncSetup>,
    /// Explicit data directory (tests, portable mode); `None` uses the platform data dir.
    pub data_dir: Option<PathBuf>,
    /// Token file of the MCP endpoint; `None` leaves the MCP server off.
    pub mcp_token_path: Option<PathBuf>,
    /// Global config file; `None` uses the platform default.
    pub global_config: Option<PathBuf>,
    /// The editing state shared with the MCP write gate; `None` creates one per session.
    pub gate: Option<Arc<crate::editing::EditingGate>>,
    /// Where MCP token secrets live besides the file (the OS keychain); `None`: in the file.
    pub mcp_secrets: Option<Arc<dyn bitacora_mcp::SecretBackend>>,
    /// MCP options from the app settings (toggles, origins, protected namespaces, rate limit).
    pub mcp: crate::settings::McpSettings,
    /// Drop the trigram block index right after opening (`search.substring = false`).
    pub disable_substring: bool,
    /// Machine-local `pando.json`; `None` leaves the Pando integration off. The session builds
    /// its `PandoOptions` from it at open (consent, mode, endpoints, features).
    pub pando_settings_path: Option<PathBuf>,
}

enum Control {
    Shutdown(Duration, mpsc::Sender<ShutdownReport>),
    /// Run a closure on the session thread (it owns the `Session`).
    Call(Box<dyn FnOnce(&Session) + Send>),
    /// The owner is gone: shut down with the default budget.
    Stop,
}

/// Cloneable handle that runs closures against the session on its own thread, so views can
/// call `Session` methods (sync actions, history, conflict resolution) without owning it.
#[derive(Clone)]
pub struct SessionHandle {
    tx: mpsc::Sender<Control>,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionHandle")
    }
}

impl SessionHandle {
    /// Runs `f` on the session thread and returns the receiver of its result. The receiver
    /// closes without a value when the session is gone. Long calls delay the event stream, so
    /// keep them to what the user waits for anyway.
    pub fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Session) -> T + Send + 'static,
    ) -> Receiver<T> {
        let (tx, rx) = async_channel::bounded(1);
        let call = Control::Call(Box::new(move |session| {
            let _ = tx.send_blocking(f(session));
        }));
        // A closed channel drops the closure and with it the result sender.
        let _ = self.tx.send(call);
        rx
    }
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
        mut options: SessionOptions,
    ) -> std::io::Result<(Self, Receiver<SessionEvent>)> {
        options.gate.get_or_insert_with(Default::default);
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

    /// A handle for running closures against the session from views.
    pub fn handle(&self) -> Option<SessionHandle> {
        self.control.clone().map(|tx| SessionHandle { tx })
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
        // Handles keep the channel open, so stopping is an explicit message; the thread shuts
        // down on its own.
        if let Some(control) = self.control.take() {
            let _ = control.send(Control::Stop);
        }
    }
}

fn runtime_config(
    root: &Path,
    options: &SessionOptions,
    with_mcp: bool,
    with_sync: bool,
) -> RuntimeConfig {
    let mut cfg = RuntimeConfig::new(root);
    cfg.data_dir.clone_from(&options.data_dir);
    if options.global_config.is_some() {
        cfg.global_config.clone_from(&options.global_config);
    }
    if with_sync && let Some(setup) = &options.sync {
        let mut sync = SyncOptions::new(setup.device.clone(), setup.branch.clone());
        sync.cli.clone_from(&setup.cli);
        sync.credentials.clone_from(&setup.credentials);
        let timing = setup.timing.clone().clamped();
        sync.tune = Some(Arc::new(move |ec| {
            use std::time::Duration;
            ec.commit.idle = Duration::from_secs(timing.commit_idle_secs);
            ec.commit.max = Duration::from_secs(timing.commit_max_secs);
            ec.commit.squash = timing.squash_auto_commits;
            ec.fetch_foreground = Duration::from_secs(timing.fetch_interval_secs);
            ec.fetch_background = Duration::from_secs(timing.fetch_interval_secs.saturating_mul(5));
        }));
        cfg.sync = Some(sync);
    }
    if with_mcp && let Some(token_path) = &options.mcp_token_path {
        cfg.mcp = Some(McpOptions {
            config: {
                let mut config = options.mcp.to_config();
                config.gate = options
                    .gate
                    .clone()
                    .map(|g| g as Arc<dyn bitacora_mcp::WriteGate>);
                config
            },
            token_path: token_path.clone(),
            secrets: options.mcp_secrets.clone(),
        });
    }
    if let Some(path) = &options.pando_settings_path {
        match bitacora_runtime::pando_options_from_file(path, root) {
            Ok(opts) => cfg.pando = opts,
            // A broken settings file never keeps a graph closed.
            Err(e) => tracing::warn!("Pando settings ignored: {e}"),
        }
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
    let mut with_mcp = true;
    let mut with_sync = options.sync.is_some();
    loop {
        match Session::open(runtime_config(root, options, with_mcp, with_sync)) {
            Err(RuntimeError::Mcp(e)) if with_mcp => {
                tracing::warn!("MCP endpoint unavailable: {e}");
                let _ = tx.send_blocking(SessionEvent::Notice(SessionNotice::McpUnavailable(
                    e.to_string(),
                )));
                with_mcp = false;
            }
            Err(RuntimeError::Sync(e)) if with_sync => {
                tracing::warn!("sync unavailable: {e}");
                let _ = tx.send_blocking(SessionEvent::Notice(SessionNotice::SyncUnavailable(
                    e.to_string(),
                )));
                with_sync = false;
            }
            other => return other,
        }
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
    if options.disable_substring
        && let Err(err) = session.set_substring(false)
    {
        tracing::warn!("cannot apply search.substring = false: {err}");
    }
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
        gate: options.gate.clone().unwrap_or_default(),
        lookup: session.ref_lookup(),
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
    let sync_watch = session.sync_watch();
    let mut sync_seen = None;
    if let Some(report) = session.recovery_report()
        && let Some(text) = recovery_message(report)
    {
        send(SessionEvent::Notice(SessionNotice::SyncRecovery(text)));
    }

    loop {
        loop {
            match control.try_recv() {
                Ok(Control::Shutdown(budget, reply)) => {
                    let report = session.shutdown(budget);
                    let _ = reply.send(report);
                    return;
                }
                Ok(Control::Call(call)) => call(&session),
                Ok(Control::Stop) | Err(TryRecvError::Disconnected) => {
                    let report = session.shutdown(DEFAULT_SHUTDOWN_BUDGET);
                    if !report.is_clean() {
                        tracing::warn!(?report, "graph session closed with unwritten files");
                    }
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if let Some(watch) = &sync_watch {
            let (version, view) = watch.current();
            if sync_seen != Some(version) {
                sync_seen = Some(version);
                send(SessionEvent::Sync(Box::new(view)));
            }
        }
        if let Some(rx) = &index_events {
            while let Ok(event) = rx.try_recv() {
                send(SessionEvent::Index(event));
            }
        }
        match runtime_events.recv_timeout(POLL_INTERVAL) {
            Ok(event) => {
                for out in events_for(&event) {
                    send(out);
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
    }
}

/// What the user should hear about startup recovery; `None` when it found nothing.
fn recovery_message(report: &bitacora_sync::recovery::RecoveryReport) -> Option<String> {
    use rust_i18n::t;
    if let Some(error) = &report.error {
        return Some(t!("sync.recovery.failed", error = error.to_string()).to_string());
    }
    let mut parts = Vec::new();
    if report.stale_lock_removed {
        parts.push(t!("sync.recovery.stale_lock").to_string());
    }
    if report.restored_conflicts > 0 {
        parts.push(t!("sync.recovery.restored", count = report.restored_conflicts).to_string());
    }
    if report.marker_conflicts > 0 {
        parts.push(t!("sync.recovery.marker", count = report.marker_conflicts).to_string());
    }
    if report.external_operation.is_some() {
        parts.push(t!("sync.recovery.external").to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(t!("sync.recovery.summary", parts = parts.join("; ")).to_string())
    }
}

/// The session events one runtime event turns into.
fn events_for(event: &RuntimeEvent) -> Vec<SessionEvent> {
    match event {
        RuntimeEvent::Queue(QueueEvent::PageConflicted(notice)) => {
            vec![SessionEvent::DiskConflict(Arc::clone(notice))]
        }
        RuntimeEvent::Queue(QueueEvent::PageReloaded(key)) => {
            vec![SessionEvent::DiskConflictCleared(key.clone())]
        }
        RuntimeEvent::Queue(QueueEvent::EditingBlockChanged(conflict)) => {
            vec![SessionEvent::EditingConflict(Box::new(conflict.clone()))]
        }
        other => notice_for(other)
            .map(SessionEvent::Notice)
            .into_iter()
            .collect(),
    }
}

/// Maps a runtime event to a user notice, when it deserves one.
fn notice_for(event: &RuntimeEvent) -> Option<SessionNotice> {
    match event {
        RuntimeEvent::Queue(QueueEvent::EditingBlockChanged(_)) => {
            Some(SessionNotice::EditingBlockChanged)
        }
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
            sync: None,
            gate: None,
            ..SessionOptions::default()
        }
    }

    #[test]
    fn the_session_builds_pando_options_from_the_settings_file() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let mut opts = options(&data);
        // No file configured, a missing file and a disabled integration all leave Pando off.
        assert!(
            runtime_config(g.path(), &opts, false, false)
                .pando
                .is_none()
        );
        let file = data.path().join("pando.json");
        opts.pando_settings_path = Some(file.clone());
        assert!(
            runtime_config(g.path(), &opts, false, false)
                .pando
                .is_none()
        );
        // A broken file never keeps the graph closed.
        std::fs::write(&file, "{ nope").expect("write");
        assert!(
            runtime_config(g.path(), &opts, false, false)
                .pando
                .is_none()
        );
        let mut settings = bitacora_config::PandoSettings {
            enabled: true,
            ..Default::default()
        };
        settings.save(&file).expect("save");
        let cfg = runtime_config(g.path(), &opts, false, false);
        let pando = cfg.pando.expect("pando options");
        assert!(pando.settings.enabled);
        assert_eq!(pando.graph, g.path());
        // The consent key the settings page writes is the graph path as the session sees it.
        settings.grant_consent(&g.path().to_string_lossy(), 1);
        assert!(settings.has_consent(&pando.graph_key()));
    }

    #[test]
    fn sync_timings_reach_the_engine_config_clamped() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let mut opts = options(&data);
        opts.sync = Some(SyncSetup {
            branch: "main".into(),
            device: "laptop".into(),
            cli: None,
            credentials: None,
            timing: crate::sync_prefs::SyncPrefs {
                commit_idle_secs: 1,
                commit_max_secs: 100,
                fetch_interval_secs: 60,
                squash_auto_commits: false,
                ..crate::sync_prefs::SyncPrefs::default()
            },
        });
        let cfg = runtime_config(g.path(), &opts, false, true);
        let sync = cfg.sync.expect("sync options");
        let tune = sync.tune.expect("tune hook");
        let mut ec = bitacora_sync::engine::EngineConfig::new(g.path(), "laptop", "main");
        tune(&mut ec);
        assert_eq!(
            ec.commit.idle,
            Duration::from_secs(5),
            "idle is clamped to 5 s"
        );
        assert_eq!(ec.commit.max, Duration::from_secs(100));
        assert!(!ec.commit.squash);
        assert_eq!(ec.fetch_foreground, Duration::from_secs(60));
        assert_eq!(ec.fetch_background, Duration::from_secs(300));
        assert_eq!(sync.device, "laptop");
        // Without a setup the session runs without sync.
        opts.sync = None;
        assert!(runtime_config(g.path(), &opts, false, true).sync.is_none());
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
