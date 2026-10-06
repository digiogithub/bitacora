//! [`Session`]: one running graph (ADR-024).
//!
//! Threads owned by a session: the core writer (`bitacora-writer`), the index writer and parse
//! pool, the watcher, the *pump* (`bitacora-runtime-pump`, which applies queue and watcher events
//! to the index and back to core), optionally the sync engine and the MCP server.
//!
//! Data flow:
//!
//! ```text
//! UI / MCP / sync --Request--> CommandQueue --EchoStore--> disk
//!                                  | QueueEvent
//!                                  +--> pump --> Indexer (update_path / delete)
//!                                  +--> sync engine (FileFlushed -> idle auto-commit)
//!                                  +--> RuntimeEvent subscribers
//! disk --> GraphWatcher (echo-filtered) --> pump --> Indexer (FsChange)
//!                                                +--> CommandQueue (LoadPage / CheckMissing)
//! ```

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};

use bitacora_config::global_config_path;
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, QueueError, QueueEvent, Request, Source};
use bitacora_core::write_queue::DebounceConfig;
use bitacora_index::{FsChange, Indexer, ReconcileStats};
use bitacora_mcp::McpConfig;
use bitacora_sync::GitDetection;
use bitacora_sync::engine::{EngineConfig, Timing};
use bitacora_watch::{FileEvent, FileEventKind, WatchConfig, WatchEvent, WatchNotice};

/// Why a session operation failed.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The graph folder is unusable.
    #[error("graph folder {path}: {source}")]
    Graph {
        /// Folder.
        path: PathBuf,
        /// Cause.
        source: std::io::Error,
    },
    /// The index could not be opened or built.
    #[error("index: {0}")]
    Index(#[from] bitacora_index::Error),
    /// The watcher could not start.
    #[error("watcher: {0}")]
    Watch(#[from] bitacora_watch::Error),
    /// The MCP server could not start.
    #[error("mcp: {0}")]
    Mcp(#[from] bitacora_mcp::Error),
    /// The sync engine could not start (e.g. the graph is not a git repository).
    #[error("sync: {0}")]
    Sync(String),
    /// Page history could not be read.
    #[error("history: {0}")]
    History(String),
    /// A restore could not be applied.
    #[error("restore: {0}")]
    Restore(String),
    /// A command queue request failed.
    #[error(transparent)]
    Queue(#[from] QueueError),
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// File.
        path: String,
        /// Cause.
        source: std::io::Error,
    },
    /// An invalid graph-relative path.
    #[error("invalid graph path `{0}`")]
    BadPath(String),
}

/// MCP endpoint settings.
#[derive(Debug, Clone)]
pub struct McpOptions {
    /// Server settings.
    pub config: McpConfig,
    /// Token file.
    pub token_path: PathBuf,
}

/// Type of the optional engine tuning hook.
pub type EngineTune = Arc<dyn Fn(&mut EngineConfig) + Send + Sync>;

/// Git sync settings (`sync.*`).
#[derive(Clone)]
pub struct SyncOptions {
    /// `Bitacora-Device` trailer value.
    pub device: String,
    /// Branch to sync.
    pub branch: String,
    /// Adjusts the engine config (timings, remote name).
    pub tune: Option<EngineTune>,
    /// Clock override (tests).
    pub timing: Option<Arc<dyn Timing>>,
    /// Git detection result; `None` runs [`detect_git`].
    pub detection: Option<GitDetection>,
    /// Start the background engine (auto-commit, periodic fetch). `false` only configures
    /// [`Session::sync_once`].
    pub background: bool,
}

impl std::fmt::Debug for SyncOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncOptions")
            .field("device", &self.device)
            .field("branch", &self.branch)
            .finish_non_exhaustive()
    }
}

impl SyncOptions {
    /// Defaults: this host, branch `main`, background engine on.
    #[must_use]
    pub fn new(device: impl Into<String>, branch: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            branch: branch.into(),
            tune: None,
            timing: None,
            detection: None,
            background: true,
        }
    }
}

/// Everything a session needs to start.
#[derive(Clone)]
pub struct RuntimeConfig {
    /// Graph folder.
    pub graph: PathBuf,
    /// Directory holding the index (default: the platform data directory).
    pub data_dir: Option<PathBuf>,
    /// Global config file merged under `logseq/config.edn`.
    pub global_config: Option<PathBuf>,
    /// Run the full reconcile while opening (default `true`).
    pub reconcile: bool,
    /// Watch the graph folder (default on; `None` disables, e.g. for one-shot commands).
    pub watch: Option<WatchConfig>,
    /// Core write debounce (`None` writes only on flush; default 400 ms / 2 s).
    pub debounce: Option<DebounceConfig>,
    /// MCP endpoint (default off).
    pub mcp: Option<McpOptions>,
    /// Git sync (default off).
    pub sync: Option<SyncOptions>,
}

impl std::fmt::Debug for RuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeConfig")
            .field("graph", &self.graph)
            .field("mcp", &self.mcp.is_some())
            .field("sync", &self.sync)
            .finish_non_exhaustive()
    }
}

impl RuntimeConfig {
    /// Defaults for `graph`: platform global config and data dir, reconcile and watch on, no MCP,
    /// no sync.
    #[must_use]
    pub fn new(graph: impl Into<PathBuf>) -> Self {
        Self {
            graph: graph.into(),
            data_dir: None,
            global_config: global_config_path(),
            reconcile: true,
            watch: Some(WatchConfig::default()),
            debounce: Some(DebounceConfig::default()),
            mcp: None,
            sync: None,
        }
    }
}

/// Notifications for the UI or the CLI (subscribe with [`Session::subscribe`]).
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// Core event (conflicts, write failures, reloaded pages...).
    Queue(QueueEvent),
    /// The watcher has a problem to report (e.g. polling fallback).
    Watch(WatchNotice),
    /// An external change was applied to the index (and to core when the page is loaded).
    ExternalChange {
        /// Graph-relative path.
        path: String,
        /// Whether a loaded page was reloaded from the new content.
        reloaded: bool,
    },
    /// A loaded page has unsaved edits and the file changed on disk: the conflict is reported by
    /// the next flush (`QueueEvent::Conflict`); nothing was overwritten.
    ExternalChangeWhileDirty {
        /// Graph-relative path.
        path: String,
    },
    /// `logseq/config.edn` changed (the config is not hot-reloaded by the runtime yet).
    ConfigChanged,
    /// A full reconcile ran after the watcher lost events.
    Reconciled(ReconcileStats),
    /// A file could not be indexed.
    IndexError {
        /// Graph-relative path.
        path: String,
        /// Message.
        message: String,
    },
}

#[derive(Clone, Default)]
pub(crate) struct Events(Arc<Mutex<Vec<Sender<RuntimeEvent>>>>);

impl Events {
    pub(crate) fn subscribe(&self) -> Receiver<RuntimeEvent> {
        let (tx, rx) = mpsc::channel();
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(tx);
        rx
    }

    pub(crate) fn emit(&self, ev: &RuntimeEvent) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|tx| tx.send(ev.clone()).is_ok());
    }
}

pub(crate) enum Job {
    Queue(QueueEvent),
    Watch(WatchEvent),
    Stop,
}

fn is_page_file(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".org")
}

/// Applies queue and watcher events to the index and, for external changes, back to core.
pub(crate) struct Pump {
    pub(crate) root: PathBuf,
    pub(crate) indexer: Arc<Indexer>,
    pub(crate) queue: CommandQueue,
    pub(crate) events: Events,
}

impl Pump {
    pub(crate) fn run(&self, rx: &Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            match job {
                Job::Queue(ev) => self.queue_event(&ev),
                Job::Watch(ev) => self.watch_event(ev),
                Job::Stop => break,
            }
        }
    }

    fn index_modified(&self, path: &GraphPath) {
        if let Err(e) = self.indexer.update_path(path) {
            self.index_error(path.as_str(), &e);
        }
    }

    fn index_deleted(&self, path: &GraphPath) {
        if let Err(e) = self.indexer.handle(&FsChange::Deleted(path.clone())) {
            self.index_error(path.as_str(), &e);
        }
    }

    fn index_error(&self, path: &str, e: &bitacora_index::Error) {
        tracing::warn!(%path, error = %e, "index update failed");
        self.events.emit(&RuntimeEvent::IndexError {
            path: path.to_owned(),
            message: e.to_string(),
        });
    }

    fn queue_event(&self, ev: &QueueEvent) {
        match ev {
            QueueEvent::Flushed(report) => {
                for w in &report.written {
                    self.index_modified(&w.path);
                }
                for d in &report.deleted {
                    self.index_deleted(d);
                }
            }
            QueueEvent::FilesApplied { written, deleted } => {
                for w in written {
                    self.index_modified(&w.path);
                }
                for d in deleted {
                    self.index_deleted(d);
                }
            }
            _ => {}
        }
    }

    /// The loaded page backed by `path`, if any.
    fn loaded_page(&self, path: &GraphPath) -> Option<PageKey> {
        self.queue.snapshot_keys().into_iter().find(|k| {
            self.queue
                .snapshot(k)
                .is_some_and(|s| s.path.as_ref() == Some(path))
        })
    }

    /// Reloads the loaded page `key` from `bytes`. Returns `Some(true)` when reloaded,
    /// `Some(false)` when it has unsaved edits, `None` on failure.
    fn reload(&self, key: &PageKey, path: &GraphPath, bytes: Vec<u8>) -> Option<bool> {
        let title = self.queue.snapshot(key)?.title.clone();
        let req = Request::LoadPage {
            key: key.clone(),
            title,
            path: Some(path.clone()),
            bytes,
        };
        match self.queue.execute(Source::External, req) {
            Ok(_) => Some(true),
            Err(QueueError::PageDirty(_)) => Some(false),
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "reload of external change failed");
                None
            }
        }
    }

    fn watch_event(&self, ev: WatchEvent) {
        match ev {
            WatchEvent::File(f) => self.file_event(&f),
            WatchEvent::Rescan => self.rescan(),
            WatchEvent::Notice(n) => self.events.emit(&RuntimeEvent::Watch(n)),
        }
    }

    fn file_event(&self, f: &FileEvent) {
        let Ok(path) = GraphPath::new(&f.rel_path) else {
            return;
        };
        if !is_page_file(&f.rel_path) {
            if f.rel_path == "logseq/config.edn" {
                self.events.emit(&RuntimeEvent::ConfigChanged);
            }
            return;
        }
        match &f.kind {
            FileEventKind::Upserted => {
                self.index_modified(&path);
                self.external_upsert(&path, f);
            }
            FileEventKind::Removed => {
                self.index_deleted(&path);
                self.check_missing(vec![path]);
            }
            FileEventKind::Renamed { from } => {
                if let Ok(from) = GraphPath::new(from) {
                    if let Err(e) = self.indexer.handle(&FsChange::Renamed {
                        from: from.clone(),
                        to: path.clone(),
                    }) {
                        self.index_error(path.as_str(), &e);
                    }
                    self.check_missing(vec![from]);
                } else {
                    self.index_modified(&path);
                }
                self.external_upsert(&path, f);
            }
        }
    }

    fn external_upsert(&self, path: &GraphPath, f: &FileEvent) {
        let reloaded = match (self.loaded_page(path), f.bytes.as_ref()) {
            (Some(key), Some(bytes)) => self.reload(&key, path, bytes.to_vec()),
            _ => Some(false),
        };
        match reloaded {
            Some(true) => self.events.emit(&RuntimeEvent::ExternalChange {
                path: path.to_string(),
                reloaded: true,
            }),
            Some(false) if self.loaded_page(path).is_some() => {
                self.events.emit(&RuntimeEvent::ExternalChangeWhileDirty {
                    path: path.to_string(),
                });
            }
            _ => self.events.emit(&RuntimeEvent::ExternalChange {
                path: path.to_string(),
                reloaded: false,
            }),
        }
    }

    fn check_missing(&self, paths: Vec<GraphPath>) {
        if let Err(e) = self
            .queue
            .execute(Source::External, Request::CheckMissing { paths })
        {
            tracing::warn!(error = %e, "CheckMissing failed");
        }
    }

    /// Events were lost: reconcile the index and refresh every loaded page without unsaved edits.
    fn rescan(&self) {
        match self.indexer.handle(&FsChange::Overflow) {
            Ok(Some(stats)) => self.events.emit(&RuntimeEvent::Reconciled(stats)),
            Ok(None) => {}
            Err(e) => self.index_error("<rescan>", &e),
        }
        let root = self.root.clone();
        for key in self.queue.snapshot_keys() {
            let Some(snap) = self.queue.snapshot(&key) else {
                continue;
            };
            let Some(path) = snap.path.clone() else {
                continue;
            };
            if snap.dirty {
                continue;
            }
            match std::fs::read(path.to_fs_path(&root)) {
                Ok(bytes) => {
                    self.reload(&key, &path, bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.check_missing(vec![path]);
                }
                Err(e) => tracing::warn!(path = %path, error = %e, "rescan read failed"),
            }
        }
    }
}
