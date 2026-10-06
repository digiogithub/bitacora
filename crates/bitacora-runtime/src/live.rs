//! The [`Session`] object: owns every running component of one graph and stops them in order.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bitacora_config::EffectiveConfig;
use bitacora_core::editor::{FlushReport, FsStore, Workspace};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::naming::derive_title;
use bitacora_core::queue::{CommandQueue, QueueConfig, QueueEvent, QueueJoin, Request, Source};
use bitacora_index::{
    Index, IndexLocation, Indexer, IndexerOptions, OpenOptions, PooledReader, ReconcileStats,
};
use bitacora_mcp::{IndexGraphReader, McpServer, TokenStore};
use bitacora_sync::engine::{
    Command as SyncCommand, EngineConfig, EngineHandle, SyncEngine, SystemTiming, Timing,
};
use bitacora_sync::state::{MemoryMergeStore, MergeStateStore, SyncState, SyncStatus};
use bitacora_sync::store::JsonMergeStore;
use bitacora_sync::{CliConfig, detect_git, select_backend};
use bitacora_watch::{EchoFilter, GraphWatcher, IgnoreRules};

use crate::glue::{SlotStatus, block_locator, journal_template_text};
use crate::session::{Events, Job, Pump, RuntimeConfig, RuntimeError, RuntimeEvent, SyncOptions};
use crate::store::EchoStore;
use crate::writer::QueueGraphWriter;

/// Time budget of [`Session::shutdown`] when the caller has no opinion.
pub const DEFAULT_SHUTDOWN_BUDGET: Duration = Duration::from_secs(10);

/// What [`Session::shutdown`] found.
#[derive(Debug, Default)]
pub struct ShutdownReport {
    /// Final state of the sync engine (`None` when sync was not running or timed out).
    pub sync: Option<SyncState>,
    /// Report of the final flush. Pages in `conflicts` / `failed` still hold unsaved edits.
    pub flush: Option<FlushReport>,
    /// Steps that did not finish within the budget (their threads were left to finish alone).
    pub timed_out: Vec<&'static str>,
}

impl ShutdownReport {
    /// Everything was written and every step finished in time.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.timed_out.is_empty() && self.flush.as_ref().is_some_and(FlushReport::is_complete)
    }
}

type SyncSlot = Arc<Mutex<Option<EngineHandle>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running graph: command queue, index, watcher and optionally sync and MCP.
pub struct Session {
    root: PathBuf,
    config: EffectiveConfig,
    index: Index,
    open_stats: Option<ReconcileStats>,
    indexer: Option<Arc<Indexer>>,
    queue: CommandQueue,
    join: Option<QueueJoin>,
    jobs: Sender<Job>,
    pump: Option<JoinHandle<()>>,
    watcher: Option<GraphWatcher>,
    mcp: Option<McpServer>,
    sync: SyncSlot,
    sync_options: Option<SyncOptions>,
    events: Events,
    stopped: bool,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

fn graph_name(root: &Path) -> String {
    root.file_name()
        .map_or_else(|| "graph".to_owned(), |n| n.to_string_lossy().into_owned())
}

fn build_engine(
    root: &Path,
    queue: &CommandQueue,
    opts: &SyncOptions,
    config: &EffectiveConfig,
    index: &Index,
) -> Result<SyncEngine, RuntimeError> {
    let detection = opts.detection.clone().unwrap_or_else(|| detect_git(None));
    let backend = select_backend(&detection, root, CliConfig::default())
        .map_err(|e| RuntimeError::Sync(e.to_string()))?;
    let mut ec = EngineConfig::new(root, &opts.device, &opts.branch);
    if let Some(tune) = &opts.tune {
        tune(&mut ec);
    }
    let writer = Arc::new(QueueGraphWriter::new(queue.clone()));
    let timing: Arc<dyn Timing> = opts
        .timing
        .clone()
        .unwrap_or_else(|| Arc::new(SystemTiming));
    // Conflict state survives restarts when the graph has a git directory.
    let store: Box<dyn MergeStateStore> = match JsonMergeStore::for_graph(root) {
        Some(s) => Box::new(s),
        None => Box::new(MemoryMergeStore::new()),
    };
    let mut engine = SyncEngine::new(backend, writer, store, ec, timing);
    let readers = index.readers().clone();
    engine.set_block_locator(block_locator(readers.clone()));
    engine.set_journal_template(journal_template_text(config, &readers));
    Ok(engine)
}

impl Session {
    /// Opens the graph: index (+ reconcile), command queue, watcher, then the optional sync
    /// engine and MCP server.
    ///
    /// # Errors
    /// [`RuntimeError`] for any component that fails to start; everything started so far is
    /// stopped again.
    pub fn open(cfg: RuntimeConfig) -> Result<Self, RuntimeError> {
        let root = std::fs::canonicalize(&cfg.graph).map_err(|source| RuntimeError::Graph {
            path: cfg.graph.clone(),
            source,
        })?;
        let config = EffectiveConfig::load(&root, cfg.global_config.as_deref());

        // Index + reconcile.
        let location = match &cfg.data_dir {
            Some(dir) => IndexLocation::in_data_dir(dir, &root)?,
            None => IndexLocation::for_graph(&root)?,
        };
        let index = Index::open(location, OpenOptions::for_config(&root, &config))?;
        let indexer = Arc::new(Indexer::start(
            &index,
            IndexerOptions::new(&root, config.clone()),
        )?);
        let open_stats = if cfg.reconcile {
            Some(indexer.reconcile()?)
        } else {
            None
        };

        // Core queue: every write registers with the echo filter before the rename.
        let echo = EchoFilter::new();
        let store = EchoStore::new(FsStore::new(&root), echo.clone());
        let events = Events::default();
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let sync: SyncSlot = Arc::new(Mutex::new(None));
        let observer = {
            let jobs = jobs.clone();
            let events = events.clone();
            let sync = Arc::clone(&sync);
            Box::new(move |ev: &QueueEvent| {
                if matches!(ev, QueueEvent::Flushed(_))
                    && let Some(h) = lock(&sync).as_ref()
                {
                    h.send(SyncCommand::FileFlushed);
                }
                events.emit(&RuntimeEvent::Queue(ev.clone()));
                let _ = jobs.send(Job::Queue(ev.clone()));
            })
        };
        let qcfg = QueueConfig {
            observers: vec![observer],
            debounce: cfg.debounce,
            ..QueueConfig::default()
        };
        let (queue, join) = CommandQueue::spawn(Workspace::new(), Box::new(store), qcfg);

        let pump_state = Pump {
            root: root.clone(),
            indexer: Arc::clone(&indexer),
            queue: queue.clone(),
            events: events.clone(),
        };
        let pump = std::thread::Builder::new()
            .name("bitacora-runtime-pump".into())
            .spawn(move || pump_state.run(&job_rx))
            .map_err(|source| RuntimeError::Graph {
                path: root.clone(),
                source,
            })?;

        let mut session = Self {
            root: root.clone(),
            config: config.clone(),
            index,
            open_stats,
            indexer: Some(indexer),
            queue: queue.clone(),
            join: Some(join),
            jobs: jobs.clone(),
            pump: Some(pump),
            watcher: None,
            mcp: None,
            sync,
            sync_options: cfg.sync.clone(),
            events,
            stopped: false,
        };

        // From here on a failure drops `session`, which stops what was started.
        if let Some(wcfg) = cfg.watch.clone() {
            let hidden_cfg = config.clone();
            let ignore = IgnoreRules::new().with_hidden(move |p| hidden_cfg.is_hidden(p));
            let tx = Mutex::new(jobs);
            let watcher = GraphWatcher::start(&root, ignore, echo, wcfg, move |ev| {
                let _ = lock(&tx).send(Job::Watch(ev));
            })?;
            session.watcher = Some(watcher);
        }
        if let Some(opts) = cfg.sync.as_ref().filter(|o| o.background) {
            let engine = build_engine(&root, &queue, opts, &config, &session.index)?;
            let handle = bitacora_sync::engine::spawn(engine);
            *lock(&session.sync) = Some(handle);
        }
        if let Some(m) = cfg.mcp {
            let tokens = Arc::new(TokenStore::load_or_init(&m.token_path)?);
            let reader = IndexGraphReader::new(&session.index, root.clone(), graph_name(&root));
            if let Some(ix) = session.indexer.as_ref() {
                reader.forward_events(ix.subscribe());
            }
            let status = Arc::new(SlotStatus(Arc::clone(&session.sync)));
            session.mcp = Some(McpServer::start_with_sync(
                m.config,
                Arc::new(reader),
                status,
                tokens,
            )?);
        }
        Ok(session)
    }

    /// Canonical graph folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Effective config the session started with.
    #[must_use]
    pub fn config(&self) -> &EffectiveConfig {
        &self.config
    }

    /// The single-writer command queue (clone it to share).
    #[must_use]
    pub fn queue(&self) -> &CommandQueue {
        &self.queue
    }

    /// A read-only index connection.
    ///
    /// # Errors
    /// [`RuntimeError::Index`] when the pool cannot hand out a connection.
    pub fn index_reader(&self) -> Result<PooledReader, RuntimeError> {
        Ok(self.index.reader()?)
    }

    /// Typed read API over the index (cheap to clone; for UIs that read pages and outlines).
    #[must_use]
    pub fn read_api(&self) -> bitacora_index::IndexReader {
        self.index.read_api()
    }

    /// New subscription to index changes (events before the call are not replayed). `None` once
    /// the session is shutting down.
    #[must_use]
    pub fn index_events(&self) -> Option<Receiver<bitacora_index::IndexEvent>> {
        self.indexer.as_ref().map(|ix| ix.subscribe())
    }

    /// Stats of the reconcile run by [`Session::open`].
    #[must_use]
    pub fn open_stats(&self) -> Option<&ReconcileStats> {
        self.open_stats.as_ref()
    }

    /// New subscription to [`RuntimeEvent`]s (events before the call are not replayed).
    #[must_use]
    pub fn subscribe(&self) -> Receiver<RuntimeEvent> {
        self.events.subscribe()
    }

    /// The MCP endpoint URL, when the server runs.
    #[must_use]
    pub fn mcp_endpoint(&self) -> Option<String> {
        self.mcp.as_ref().map(McpServer::endpoint)
    }

    /// Whether the watcher fell back to polling (`None` without a watcher).
    #[must_use]
    pub fn watcher_is_polling(&self) -> Option<bool> {
        self.watcher.as_ref().map(GraphWatcher::is_polling)
    }

    /// Status of the background sync engine.
    #[must_use]
    pub fn sync_status(&self) -> Option<SyncStatus> {
        lock(&self.sync).as_ref().map(EngineHandle::status)
    }

    /// Asks the background engine for a sync cycle ("Sync now"); returns immediately.
    /// Returns `false` when no engine runs.
    pub fn sync_now(&self) -> bool {
        match lock(&self.sync).as_ref() {
            Some(h) => {
                h.send(SyncCommand::SyncNow);
                true
            }
            None => false,
        }
    }

    /// Runs one complete sync cycle on the calling thread (one-shot `sync`): builds an engine
    /// from the session's [`SyncOptions`] over the queue writer and returns its final state.
    ///
    /// # Errors
    /// [`RuntimeError::Sync`] when sync is not configured or the backend cannot start.
    pub fn sync_once(&self) -> Result<(SyncState, SyncStatus), RuntimeError> {
        let opts = self
            .sync_options
            .as_ref()
            .ok_or_else(|| RuntimeError::Sync("sync is not configured".into()))?;
        let mut engine = build_engine(&self.root, &self.queue, opts, &self.config, &self.index)?;
        let state = engine.sync_now();
        Ok((state, engine.status()))
    }

    /// Loads the page stored in `rel` (graph-relative) into core so it can be edited.
    ///
    /// # Errors
    /// [`RuntimeError::Read`], [`RuntimeError::BadPath`] or a queue error.
    pub fn open_page(&self, rel: &str) -> Result<PageKey, RuntimeError> {
        let path = GraphPath::new(rel).map_err(|_| RuntimeError::BadPath(rel.to_owned()))?;
        let bytes =
            std::fs::read(path.to_fs_path(&self.root)).map_err(|source| RuntimeError::Read {
                path: rel.to_owned(),
                source,
            })?;
        let title = derive_title(rel, None, &self.config);
        let key = PageKey::from_title(&title);
        self.queue.execute(
            Source::Ui,
            Request::LoadPage {
                key: key.clone(),
                title,
                path: Some(path),
                bytes,
            },
        )?;
        Ok(key)
    }

    /// Ordered shutdown within `budget`: sync (final commit and push), flush of every dirty
    /// page, watcher, index drain, MCP.
    pub fn shutdown(mut self, budget: Duration) -> ShutdownReport {
        self.stop_all(budget)
    }

    fn stop_all(&mut self, budget: Duration) -> ShutdownReport {
        let mut report = ShutdownReport::default();
        if std::mem::replace(&mut self.stopped, true) {
            return report;
        }
        let deadline = Instant::now() + budget;
        let left = || deadline.saturating_duration_since(Instant::now());

        // 1. Sync: final commit + best-effort push while the writer still runs.
        let handle = lock(&self.sync).take();
        if let Some(handle) = handle {
            let (tx, rx) = mpsc::channel();
            handle.send(SyncCommand::Close(tx));
            match rx.recv_timeout(left()) {
                Ok(state) => report.sync = Some(state),
                Err(_) => report.timed_out.push("sync"),
            }
            // Joining may outlive the budget when the engine is mid-cycle: do it off-thread.
            let _ = std::thread::Builder::new()
                .name("bitacora-sync-join".into())
                .spawn(move || drop(handle));
        }

        // 2. Core: flush and stop the writer.
        if let Some(join) = self.join.take() {
            let (tx, rx) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("bitacora-flush".into())
                .spawn(move || {
                    let _ = tx.send(join.shutdown_with_report());
                });
            match (spawned, rx.recv_timeout(left())) {
                (Ok(_), Ok(Ok((_ws, flush)))) => report.flush = Some(flush),
                (Ok(_), Ok(Err(e))) => {
                    tracing::warn!(error = %e, "writer thread panicked during shutdown");
                }
                _ => report.timed_out.push("flush"),
            }
        }

        // 3. Watcher.
        self.watcher = None;

        // 4. Index: apply the events the final flush produced, then drain the writer.
        let _ = self.jobs.send(Job::Stop);
        if let Some(pump) = self.pump.take() {
            let (tx, rx) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("bitacora-pump-join".into())
                .spawn(move || {
                    let _ = pump.join();
                    let _ = tx.send(());
                });
            if spawned.is_err() || rx.recv_timeout(left()).is_err() {
                report.timed_out.push("index");
            }
        }
        if let Some(indexer) = self.indexer.take() {
            match Arc::try_unwrap(indexer) {
                Ok(i) => i.shutdown(),
                Err(_) => report.timed_out.push("index"),
            }
        }

        // 5. MCP.
        if let Some(mcp) = self.mcp.take() {
            mcp.stop();
        }
        report
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stop_all(DEFAULT_SHUTDOWN_BUDGET);
    }
}
