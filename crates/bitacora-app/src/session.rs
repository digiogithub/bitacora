//! `GraphSession`: opens a graph's index and reconciles it on a background thread.
//!
//! The session owns one std thread that opens the SQLite index (outside the graph, ADR-005),
//! starts the [`Indexer`] and runs the startup reconcile, reporting [`SessionEvent`]s through an
//! `async_channel` that a GPUI view drains with [`crate::events::EventPump`] or a spawned
//! task. Dropping the session tells the thread to shut the indexer down; [`GraphSession::close`]
//! additionally waits for it, which closes the index cleanly before another graph is opened.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_channel::{Receiver, Sender};
use bitacora_config::{EffectiveConfig, global_config_path};
use bitacora_core::date::Date;
use bitacora_core::scan::{parse_order, scan_graph};
use bitacora_index::{
    Index, IndexEvent, IndexLocation, Indexer, IndexerOptions, OpenOptions, ReconcileStats,
    config_hash,
};

use crate::data::{GraphHandle, ViewSettings};

/// Minimum time between two progress events.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// What the session thread reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// The index is open. `rebuilt` is true when a corrupt or outdated database was recreated.
    Opened {
        /// A database file was deleted and recreated.
        rebuilt: bool,
        /// Files the reconcile will look at.
        total: usize,
    },
    /// The read API of the index is available (pages may still be incomplete until `Ready`).
    Reader(GraphHandle),
    /// The index changed after the startup reconcile finished (file edits, sync, rename).
    Index(IndexEvent),
    /// Files indexed so far during the reconcile.
    Progress {
        /// Files written.
        done: usize,
        /// Files found by the scan.
        total: usize,
    },
    /// The reconcile finished; the graph is browsable and kept up to date.
    Ready(SessionSummary),
    /// Opening or reconciling failed; the message is user-presentable.
    Failed(String),
}

/// Result of the startup reconcile.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Where the session keeps its index.
#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    /// Explicit data directory (tests, portable mode); `None` uses the platform data dir.
    pub data_dir: Option<PathBuf>,
}

/// A running graph session.
#[derive(Debug)]
pub struct GraphSession {
    root: PathBuf,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl GraphSession {
    /// Starts the session thread and returns the receiver of its events.
    pub fn start(
        root: PathBuf,
        options: SessionOptions,
    ) -> std::io::Result<(Self, Receiver<SessionEvent>)> {
        let (tx, rx) = async_channel::unbounded();
        let (stop_tx, stop_rx) = mpsc::channel();
        let thread_root = root.clone();
        let thread = std::thread::Builder::new()
            .name("bitacora-session".into())
            .spawn(move || run(&thread_root, &options, &tx, &stop_rx))?;
        Ok((
            Self {
                root,
                stop: Some(stop_tx),
                thread: Some(thread),
            },
            rx,
        ))
    }

    /// The graph folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stops the indexer and waits until the index is closed.
    pub fn close(mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            // A panicked session thread has nothing left to clean up.
            let _ = thread.join();
        }
    }
}

impl Drop for GraphSession {
    fn drop(&mut self) {
        // Closing the channel is the stop signal; the thread cleans up on its own.
        self.stop.take();
    }
}

fn run(
    root: &Path,
    options: &SessionOptions,
    tx: &Sender<SessionEvent>,
    stop: &mpsc::Receiver<()>,
) {
    match open_and_reconcile(root, options, tx) {
        Ok(Some(indexer)) => {
            // Wait for the stop signal (sender dropped), then close the index.
            let _ = stop.recv();
            indexer.shutdown();
        }
        Ok(None) => {}
        Err(message) => {
            tracing::error!(graph = %root.display(), "graph session failed: {message}");
            let _ = tx.send_blocking(SessionEvent::Failed(message));
        }
    }
}

fn open_and_reconcile(
    root: &Path,
    options: &SessionOptions,
    tx: &Sender<SessionEvent>,
) -> Result<Option<Indexer>, String> {
    // The receiver is gone when the view was closed: nothing left to report to.
    let send = |event| {
        let _ = tx.send_blocking(event);
    };
    let cfg = EffectiveConfig::load(root, global_config_path().as_deref());
    let location = match &options.data_dir {
        Some(dir) => IndexLocation::in_data_dir(dir, root),
        None => IndexLocation::for_graph(root),
    }
    .map_err(|e| format!("cannot locate the index: {e}"))?;
    let index = Index::open(location, OpenOptions::new(root, config_hash(&cfg)))
        .map_err(|e| format!("cannot open the index: {e}"))?;
    let rebuilt = index.outcome().recreated.is_some();

    let total = scan_graph(root, &cfg)
        .map(|files| parse_order(&files).len())
        .map_err(|e| format!("cannot scan the graph: {e}"))?;
    send(SessionEvent::Opened { rebuilt, total });
    send(SessionEvent::Reader(GraphHandle {
        reader: index.read_api(),
        root: root.to_path_buf(),
        settings: Arc::new(ViewSettings::from_config(&cfg)),
    }));

    let mut opts = IndexerOptions::new(root, cfg);
    opts.today = today_utc();
    let indexer =
        Indexer::start(&index, opts).map_err(|e| format!("cannot start indexing: {e}"))?;
    let events = indexer.subscribe();
    let progress_tx = tx.clone();
    let finished = Arc::new(AtomicBool::new(false));
    let counter_finished = finished.clone();
    let counter = std::thread::Builder::new()
        .name("bitacora-progress".into())
        .spawn(move || count_progress(&events, total, &progress_tx, &counter_finished))
        .map_err(|e| format!("cannot start the progress thread: {e}"))?;

    let result = indexer.reconcile();
    finished.store(true, Ordering::SeqCst);
    match result {
        Ok(stats) => {
            send(SessionEvent::Progress { done: total, total });
            send(SessionEvent::Ready(SessionSummary::from(&stats)));
            // The counter ends when the writer (owned by the indexer) shuts down; it is
            // detached on purpose so that `shutdown` is the only thing that waits.
            drop(counter);
            Ok(Some(indexer))
        }
        Err(e) => {
            indexer.shutdown();
            let _ = counter.join();
            Err(format!("indexing failed: {e}"))
        }
    }
}

fn count_progress(
    events: &mpsc::Receiver<IndexEvent>,
    total: usize,
    tx: &Sender<SessionEvent>,
    finished: &AtomicBool,
) {
    let mut done = 0;
    let mut last = Instant::now();
    while let Ok(event) = events.recv() {
        if finished.load(Ordering::SeqCst) {
            // After the startup reconcile every change is forwarded to the views.
            let _ = tx.send_blocking(SessionEvent::Index(event));
            continue;
        }
        if matches!(event, IndexEvent::FileReplaced { .. }) {
            done += 1;
            if last.elapsed() >= PROGRESS_INTERVAL {
                last = Instant::now();
                let _ = tx.send_blocking(SessionEvent::Progress {
                    done: done.min(total),
                    total,
                });
            }
        }
    }
}

/// Today's date in UTC (a priority hint for the indexer only).
fn today_utc() -> Option<Date> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let days = i64::try_from(secs / 86_400).ok()?;
    // Civil-from-days (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u8::try_from(doy - (153 * mp + 2) / 5 + 1).ok()?;
    let month = u8::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).ok()?;
    let year = i32::try_from(yoe + era * 400 + i64::from(month <= 2)).ok()?;
    Date::new(year, month, day)
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

    fn collect(rx: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
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
    fn session_indexes_the_graph_and_closes_cleanly() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("start");
        let events = collect(&rx);
        assert!(
            matches!(
                events.first(),
                Some(SessionEvent::Opened {
                    rebuilt: false,
                    total: 5
                })
            ),
            "{events:?}"
        );
        let Some(SessionEvent::Ready(summary)) = events.last() else {
            panic!("expected Ready, got {events:?}");
        };
        assert_eq!(summary.scanned, 5);
        assert_eq!(summary.parsed, 5);
        assert!(summary.cold_build);
        session.close();

        // Reopening the same graph is a warm start: nothing to parse.
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("restart");
        let events = collect(&rx);
        let Some(SessionEvent::Ready(summary)) = events.last() else {
            panic!("expected Ready, got {events:?}");
        };
        assert_eq!(summary.parsed, 0);
        session.close();
    }

    #[test]
    fn corrupt_index_is_rebuilt_and_reported() {
        let g = graph();
        let data = tempfile::tempdir().expect("data");
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("start");
        collect(&rx);
        session.close();
        let location = IndexLocation::in_data_dir(data.path(), g.path()).expect("location");
        let db = location.db_path();
        for suffix in ["-wal", "-shm"] {
            let mut side = db.clone().into_os_string();
            side.push(suffix);
            let _ = std::fs::remove_file(side);
        }
        std::fs::write(&db, b"this is not a database, just garbage bytes").expect("corrupt");
        let (session, rx) =
            GraphSession::start(g.path().to_path_buf(), options(&data)).expect("restart");
        let events = collect(&rx);
        assert!(
            matches!(
                events.first(),
                Some(SessionEvent::Opened { rebuilt: true, .. })
            ),
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
        let events = collect(&rx);
        assert!(
            matches!(events.last(), Some(SessionEvent::Failed(_))),
            "{events:?}"
        );
        session.close();
    }

    #[test]
    fn today_is_a_plausible_date() {
        let today = today_utc().expect("date");
        assert!(today.year() >= 2024);
    }
}
