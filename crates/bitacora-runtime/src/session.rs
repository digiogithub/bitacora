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

use crate::glue::journal_template_text;
use bitacora_config::{EffectiveConfig, global_config_path};
use bitacora_core::editor::{EditorSettings, ExternalOutcome};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, QueueError, QueueEvent, Request, Response, Source};
use bitacora_core::write_queue::DebounceConfig;
use bitacora_index::{FsChange, Indexer, ReaderPool, RebuildKind, ReconcileStats};
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
    /// Where token secrets live besides the file (the OS keychain); `None` keeps them in the
    /// `0600` file.
    pub secrets: Option<Arc<dyn bitacora_mcp::SecretBackend>>,
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
    /// System-git tunables (askpass helper and its environment, timeouts); `None` uses the
    /// defaults, which leave the system backend without an askpass helper (BIT-US-0046).
    pub cli: Option<bitacora_sync::CliConfig>,
    /// Credential provider for pushes without system git (gix-only backend, ADR-023).
    pub credentials: Option<Arc<dyn bitacora_sync::credentials::CredentialProvider>>,
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
            cli: None,
            credentials: None,
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
    /// A loaded page has unsaved edits and the file changed on disk in a way that cannot be
    /// merged: ours is kept, writes to the page are stopped and the notice data arrives as
    /// `QueueEvent::PageConflicted` (BIT-US-0070); nothing was overwritten.
    ExternalChangeWhileDirty {
        /// Graph-relative path.
        path: String,
    },
    /// External edits were merged block by block into a loaded page with unsaved edits; the
    /// merged page is written by the normal pipeline (BIT-US-0069).
    ExternalMerged {
        /// Graph-relative path.
        path: String,
    },
    /// `logseq/config.edn` changed on disk. Always followed by [`RuntimeEvent::ConfigReloaded`]
    /// or [`RuntimeEvent::ConfigReloadFailed`].
    ConfigChanged,
    /// The effective config was reloaded and applied to core (editor settings), the watcher's
    /// hidden rules and the index; [`Session::current_config`] returns it. The UI should refresh
    /// whatever it derived from the config (view settings, journal formats, file name format).
    ConfigReloaded {
        /// The new effective config.
        config: Arc<EffectiveConfig>,
        /// The change affects what the parser produces: the index was fully rebuilt.
        reindexed: bool,
    },
    /// `logseq/config.edn` is not valid EDN (or unreadable): the previous config stays active.
    ConfigReloadFailed {
        /// First diagnostic.
        message: String,
    },
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
    /// Rebuild the whole index; the result goes back through the channel.
    Reindex(Sender<Result<ReconcileStats, bitacora_index::Error>>),
    Stop,
}

fn is_page_file(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".org")
}

/// The effective config shared by the session, the watcher's hidden rule and the pump; replaced
/// as a whole on reload.
pub(crate) type SharedConfig = Arc<std::sync::RwLock<Arc<EffectiveConfig>>>;

pub(crate) fn current(c: &SharedConfig) -> Arc<EffectiveConfig> {
    Arc::clone(&c.read().unwrap_or_else(PoisonError::into_inner))
}

/// Applies queue and watcher events to the index and, for external changes, back to core.
pub(crate) struct Pump {
    pub(crate) root: PathBuf,
    pub(crate) config: SharedConfig,
    pub(crate) global_config: Option<PathBuf>,
    pub(crate) readers: ReaderPool,
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
                Job::Reindex(reply) => {
                    let _ = reply.send(self.indexer.reindex());
                }
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

    /// Applies the new `bytes` of the loaded page `key` (reload with stable block ids, or a
    /// block-level merge when it has unsaved edits). `None` on failure.
    fn apply(&self, key: &PageKey, path: &GraphPath, bytes: Vec<u8>) -> Option<ExternalOutcome> {
        let req = Request::ExternalChange {
            key: key.clone(),
            bytes,
        };
        match self.queue.execute(Source::External, req) {
            Ok(Response::External(out)) => Some(out),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "external change could not be applied");
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
                self.reload_config();
            }
            return;
        }
        if self.is_template_only_journal(&path, f) {
            // A journal Logseq (or another tool) just created with the default template: nothing
            // the user wrote (BIT-SP-0002.R17), so neither the index nor core react to it.
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

    /// An upserted, not loaded journal file whose trimmed content equals the configured default
    /// journal template.
    fn is_template_only_journal(&self, path: &GraphPath, f: &FileEvent) -> bool {
        if !matches!(f.kind, FileEventKind::Upserted) {
            return false;
        }
        let cfg = current(&self.config);
        let prefix = format!("{}/", cfg.journals_directory());
        if !path.as_str().starts_with(&prefix) {
            return false;
        }
        let Some(bytes) = f.bytes.as_ref() else {
            return false;
        };
        let Some(template) = journal_template_text(&cfg, &self.readers) else {
            return false;
        };
        if self.loaded_page(path).is_some() {
            return false;
        }
        let norm = |s: &str| {
            s.trim()
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
        };
        norm(&String::from_utf8_lossy(bytes)) == norm(&template)
    }

    /// Re-reads the effective config and applies it everywhere the runtime owns state: core's
    /// editor settings, the index (config hash, full reparse when it changed) and, through the
    /// shared handle, the watcher's hidden rule. An invalid config keeps the previous one.
    fn reload_config(&self) {
        let old = current(&self.config);
        let new = EffectiveConfig::load(&self.root, self.global_config.as_deref());
        if new.diagnostics().len() > old.diagnostics().len()
            && let Some(d) = new.diagnostics().last()
        {
            self.events.emit(&RuntimeEvent::ConfigReloadFailed {
                message: d.to_string(),
            });
            return;
        }
        if new == *old {
            self.events.emit(&RuntimeEvent::ConfigReloaded {
                config: old,
                reindexed: false,
            });
            return;
        }
        let new = Arc::new(new);
        *self.config.write().unwrap_or_else(PoisonError::into_inner) = Arc::clone(&new);
        if let Err(e) = self
            .queue
            .set_settings(Source::External, EditorSettings::from_config(&new))
        {
            tracing::warn!(error = %e, "editor settings could not be updated");
        }
        let mut reindexed = false;
        match self.indexer.set_config((*new).clone()) {
            Ok(RebuildKind::None) => {}
            Ok(_) => match self.indexer.reconcile() {
                Ok(stats) => {
                    reindexed = true;
                    self.events.emit(&RuntimeEvent::Reconciled(stats));
                }
                Err(e) => self.index_error("logseq/config.edn", &e),
            },
            Err(e) => self.index_error("logseq/config.edn", &e),
        }
        self.events.emit(&RuntimeEvent::ConfigReloaded {
            config: new,
            reindexed,
        });
    }

    fn external_upsert(&self, path: &GraphPath, f: &FileEvent) {
        let out = match (self.loaded_page(path), f.bytes.as_ref()) {
            (Some(key), Some(bytes)) => self.apply(&key, path, bytes.to_vec()),
            _ => None,
        };
        let p = path.to_string();
        match out {
            Some(ExternalOutcome::Reloaded(_) | ExternalOutcome::Unchanged) => {
                self.events.emit(&RuntimeEvent::ExternalChange {
                    path: p,
                    reloaded: true,
                });
            }
            Some(ExternalOutcome::Merged { .. }) => {
                self.events.emit(&RuntimeEvent::ExternalChange {
                    path: p.clone(),
                    reloaded: true,
                });
                self.events.emit(&RuntimeEvent::ExternalMerged { path: p });
            }
            Some(ExternalOutcome::Conflict(_)) => {
                self.events
                    .emit(&RuntimeEvent::ExternalChangeWhileDirty { path: p });
            }
            Some(ExternalOutcome::Unknown) | None => {
                self.events.emit(&RuntimeEvent::ExternalChange {
                    path: p,
                    reloaded: false,
                });
            }
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

    /// Events were lost: reconcile the index and refresh every loaded page (unsaved edits are merged).
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
            match std::fs::read(path.to_fs_path(&root)) {
                Ok(bytes) => {
                    self.apply(&key, &path, bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.check_missing(vec![path]);
                }
                Err(e) => tracing::warn!(path = %path, error = %e, "rescan read failed"),
            }
        }
    }
}
