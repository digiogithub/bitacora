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
use bitacora_mcp::{
    AuditFilter, AuditRecord, IndexGraphReader, McpServer, QueueBridge, ServerParts, TokenStore,
    UndoError, WritePolicy,
};
use bitacora_pando::{PandoEvent, PandoService, PandoStatus};
use bitacora_sync::credentials::CredentialProvider;
use bitacora_sync::engine::{
    Command as SyncCommand, EngineConfig, EngineHandle, SyncEngine, SystemTiming, Timing,
};
use bitacora_sync::history::{
    BlockDiff, HistoryEntry, PageDiff, diff_version, page_history, version_text,
};
use bitacora_sync::merge::Resolution;
use bitacora_sync::recovery::RecoveryReport;
use bitacora_sync::resolve::{ResolveError, ResolveOutcome};
use bitacora_sync::state::{MemoryMergeStore, MergeStateStore, SyncState, SyncStatus};
use bitacora_sync::store::JsonMergeStore;
use bitacora_sync::{
    CliBackend, CliConfig, GitBackend, GitDetection, GixBackend, HybridBackend, detect_git,
    select_backend,
};
use bitacora_watch::{EchoFilter, GraphWatcher, IgnoreRules};

use crate::glue::{SlotStatus, block_locator, journal_template_text};
use crate::restore::{RestoreReport, Selection, restore};
use crate::session::{
    Events, Job, Pump, RuntimeConfig, RuntimeError, RuntimeEvent, SharedConfig, SyncOptions,
    current,
};
use crate::store::EchoStore;
use crate::sync_ctl::{BackendInfo, SyncStatusView, SyncWatch};
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

type SyncSlot = Arc<Mutex<Option<Arc<EngineHandle>>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running graph: command queue, index, watcher and optionally sync and MCP.
pub struct Session {
    root: PathBuf,
    config: EffectiveConfig,
    live_config: SharedConfig,
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
    pando: Option<PandoService>,
    /// Semantic sync of the graph into Pando's KB (ADR-030); stops before the Pando service.
    semantic: Option<bitacora_pando::semantic::SemanticWorker>,
    /// FTS5 + semantic search (BIT-US-0144); lexical only when Pando is off.
    hybrid: Option<bitacora_pando::semantic::HybridSearch>,
    sync_watch: Option<SyncWatch>,
    recovery: Option<RecoveryReport>,
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

fn backend_with_credentials(
    detection: &GitDetection,
    root: &Path,
    cli: CliConfig,
    provider: &Arc<dyn CredentialProvider>,
) -> Result<Box<dyn GitBackend>, RuntimeError> {
    let sync_err = |e: bitacora_sync::GitError| RuntimeError::Sync(e.to_string());
    let gix = GixBackend::open(root)
        .map_err(sync_err)?
        .with_credentials(Arc::clone(provider));
    Ok(match detection {
        GitDetection::Found { path, .. } => Box::new(HybridBackend::new(
            CliBackend::new(root, path.clone(), cli),
            gix,
        )),
        GitDetection::Missing | GitDetection::TooOld { .. } => Box::new(gix),
    })
}

fn build_engine(
    root: &Path,
    queue: &CommandQueue,
    opts: &SyncOptions,
    config: &EffectiveConfig,
    index: &Index,
) -> Result<(SyncEngine, SyncWatch), RuntimeError> {
    let detection = opts.detection.clone().unwrap_or_else(|| detect_git(None));
    let backend_info = BackendInfo::from_detection(&detection);
    let cli = opts.cli.clone().unwrap_or_default();
    let backend = match &opts.credentials {
        // The gix half pushes through libgit2 and needs the provider for HTTPS/SSH secrets.
        Some(provider) => backend_with_credentials(&detection, root, cli, provider)?,
        None => {
            select_backend(&detection, root, cli).map_err(|e| RuntimeError::Sync(e.to_string()))?
        }
    };
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
    let watch = SyncWatch::new(engine.status(), backend_info);
    let publisher = watch.clone();
    engine.set_observer(Arc::new(move |status: &SyncStatus| {
        publisher.publish(status);
    }));
    Ok((engine, watch))
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

        let live_config: SharedConfig = Arc::new(std::sync::RwLock::new(Arc::new(config.clone())));
        let pump_state = Pump {
            root: root.clone(),
            config: Arc::clone(&live_config),
            global_config: cfg.global_config.clone(),
            readers: index.readers().clone(),
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
            live_config: Arc::clone(&live_config),
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
            pando: None,
            semantic: None,
            hybrid: None,
            sync_watch: None,
            recovery: None,
            events,
            stopped: false,
        };

        // From here on a failure drops `session`, which stops what was started.
        if let Some(wcfg) = cfg.watch.clone() {
            // Reads the live config so a reloaded `:hidden` applies without restarting the watcher.
            let hidden_cfg = Arc::clone(&live_config);
            let ignore = IgnoreRules::new().with_hidden(move |p| current(&hidden_cfg).is_hidden(p));
            let tx = Mutex::new(jobs);
            let watcher = GraphWatcher::start(&root, ignore, echo, wcfg, move |ev| {
                let _ = lock(&tx).send(Job::Watch(ev));
            })?;
            session.watcher = Some(watcher);
        }
        if let Some(opts) = cfg.sync.as_ref().filter(|o| o.background) {
            let (mut engine, watch) = build_engine(&root, &queue, opts, &config, &session.index)?;
            // Startup recovery (BIT-US-0047): stale lock, interrupted merge, markers, pending
            // conflicts; runs before the engine thread starts so the first status is accurate.
            let report = engine.recover();
            watch.publish(&engine.status());
            session.recovery = Some(report);
            session.sync_watch = Some(watch);
            let handle = bitacora_sync::engine::spawn(engine);
            *lock(&session.sync) = Some(Arc::new(handle));
        }
        if let Some(m) = cfg.mcp {
            let tokens = Arc::new(TokenStore::load_or_init_with(
                &m.token_path,
                m.secrets.clone(),
            )?);
            let reader = IndexGraphReader::new(&session.index, root.clone(), graph_name(&root));
            if let Some(ix) = session.indexer.as_ref() {
                reader.forward_events(ix.subscribe());
            }
            let status = Arc::new(SlotStatus(Arc::clone(&session.sync)));
            let mut mcp_config = m.config;
            if mcp_config.audit_dir.is_none() {
                mcp_config.audit_dir = match &cfg.data_dir {
                    Some(dir) => Some(dir.join("mcp-audit")),
                    None => bitacora_mcp::default_audit_dir(),
                };
            }
            // Agent writes go through the same single-writer queue as the UI (rule 3).
            let writer = QueueBridge::new(queue.clone(), root.clone(), config.clone())
                .with_ref_lookup(session.ref_lookup());
            session.mcp = Some(McpServer::start_with(
                mcp_config,
                ServerParts {
                    reader: Arc::new(reader),
                    sync: status,
                    tokens,
                    writer: Some(writer),
                    gate: None,
                },
            )?);
        }
        // Pando never fails the open: a bad setup shows up as `PandoStatus::Unavailable`.
        if let Some(mut p) = cfg.pando {
            if p.graph.as_os_str().is_empty() {
                p.graph = root.clone();
            }
            if let Some(server) = session.mcp.as_ref() {
                provision_pando_mcp(server, &mut p);
            }
            if p.supervisor.is_none() && p.settings.mode == bitacora_config::PandoMode::Managed {
                p.supervisor = default_supervisor(&p.settings);
            }
            let service = PandoService::start(p.clone());
            // Semantic search is optional: a failure is logged and never stops the open.
            if let Some(ix) = session.indexer.as_ref() {
                let inputs = bitacora_pando::semantic::SessionInputs {
                    graph_root: &root,
                    data_dir: session.index.location().dir(),
                    reader: session.index.read_api(),
                    events: ix.subscribe(),
                };
                match bitacora_pando::semantic::start_session(&service, &p, inputs) {
                    Ok(worker) => session.semantic = worker,
                    Err(e) => tracing::warn!(error = %e, "semantic sync not started"),
                }
            }
            session.pando = Some(service);
        }
        match bitacora_pando::semantic::HybridSearch::for_session(
            session.pando.as_ref(),
            session.semantic.as_ref().map(|w| w.policy()),
            &root,
            session.index.read_api(),
        ) {
            Ok(h) => session.hybrid = Some(h),
            Err(e) => tracing::warn!(error = %e, "hybrid search unavailable"),
        }
        Ok(session)
    }

    /// The Pando integration of this session, when [`RuntimeConfig::pando`] was set.
    #[must_use]
    pub fn pando(&self) -> Option<&PandoService> {
        self.pando.as_ref()
    }

    /// Counts of the semantic sync (`None` when it is not running).
    #[must_use]
    pub fn semantic_status(&self) -> Option<bitacora_pando::semantic::SemanticStatus> {
        self.semantic.as_ref().map(|w| w.status())
    }

    /// Hybrid search: FTS5 results fused (RRF) with Pando's semantic results, which are
    /// re-resolved against the local index (stale, unknown and excluded hits are dropped).
    /// Without Pando (off, no consent, unreachable, timeout) the answer is lexical only and
    /// [`HybridResults::semantic`] says why.
    ///
    /// Blocks while waiting for Pando (at most `opts.timeout` plus a short margin): call it from
    /// a background thread, never from the UI thread.
    ///
    /// # Errors
    /// [`RuntimeError::Search`] when the local index cannot be searched.
    pub fn hybrid_search(
        &self,
        query: &str,
        opts: &bitacora_pando::semantic::HybridOptions,
    ) -> Result<bitacora_pando::semantic::HybridResults, RuntimeError> {
        let h = self
            .hybrid
            .as_ref()
            .ok_or_else(|| RuntimeError::Search("hybrid search is not available".into()))?;
        h.search(query, opts)
            .map_err(|e| RuntimeError::Search(e.to_string()))
    }

    /// The semantic sync worker, to change exclusions, resync or purge (BIT-SP-0010.R6).
    #[must_use]
    pub fn semantic(&self) -> Option<&bitacora_pando::semantic::SemanticWorker> {
        self.semantic.as_ref()
    }

    /// Current Pando status (`Off` when the integration was not configured).
    #[must_use]
    pub fn pando_status(&self) -> PandoStatus {
        self.pando
            .as_ref()
            .map_or(PandoStatus::Off, PandoService::status)
    }

    /// New subscription to Pando events (first message is the current status); `None` when the
    /// integration was not configured.
    #[must_use]
    pub fn pando_events(&self) -> Option<Receiver<PandoEvent>> {
        self.pando.as_ref().map(PandoService::subscribe)
    }

    /// Canonical graph folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Effective config the session started with. After a `config.edn` hot reload use
    /// [`Session::current_config`].
    #[must_use]
    pub fn config(&self) -> &EffectiveConfig {
        &self.config
    }

    /// The effective config now in force (follows `config.edn` reloads; see
    /// `RuntimeEvent::ConfigReloaded`).
    #[must_use]
    pub fn current_config(&self) -> Arc<EffectiveConfig> {
        current(&self.live_config)
    }

    /// Drops the index and rebuilds it from the graph files (settings "Reindex"). Index updates
    /// from edits and external changes are held until it finishes and then applied in order, so
    /// nothing is lost and the database file is never replaced under the readers. Returns the
    /// stats of the rebuild (also announced as `RuntimeEvent::Reconciled`).
    ///
    /// # Errors
    /// [`RuntimeError::Index`] when the rebuild fails or the session is shutting down.
    pub fn reindex(&self) -> Result<ReconcileStats, RuntimeError> {
        // Whatever is dirty goes to disk first so the rebuild sees what the user sees.
        self.queue.flush(Source::Ui)?;
        let (tx, rx) = mpsc::channel();
        self.jobs
            .send(Job::Reindex(tx))
            .map_err(|_| RuntimeError::Index(bitacora_index::Error::WriterStopped))?;
        let stats = rx
            .recv()
            .map_err(|_| RuntimeError::Index(bitacora_index::Error::WriterStopped))??;
        self.events.emit(&RuntimeEvent::Reconciled(stats.clone()));
        Ok(stats)
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

    /// Index-backed lookups for page renames (`RenameRequest::lookup`).
    #[must_use]
    pub fn ref_lookup(&self) -> std::sync::Arc<crate::IndexRefLookup> {
        std::sync::Arc::new(crate::IndexRefLookup::new(self.index.read_api()))
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

    /// The MCP write policy (agent writes / deletes toggles, protected namespaces), when the
    /// server runs. Changes take effect immediately.
    #[must_use]
    pub fn mcp_policy(&self) -> Option<Arc<WritePolicy>> {
        self.mcp.as_ref().map(|m| Arc::clone(m.policy()))
    }

    /// The bearer tokens of the running MCP server (create / revoke / rotate take effect for the
    /// next request), when the server runs.
    #[must_use]
    pub fn mcp_tokens(&self) -> Option<Arc<TokenStore>> {
        self.mcp.as_ref().map(|m| Arc::clone(m.tokens()))
    }

    /// Applies `search.substring`: drops (`false`) or builds (`true`) the trigram block index.
    /// Returns whether the index changed.
    ///
    /// # Errors
    /// [`RuntimeError::Index`] when the index cannot be changed or the session has no indexer.
    pub fn set_substring(&self, enabled: bool) -> Result<bool, RuntimeError> {
        let indexer = self
            .indexer
            .as_ref()
            .ok_or(RuntimeError::Index(bitacora_index::Error::WriterStopped))?;
        Ok(indexer.writer().set_substring(enabled)?)
    }

    /// Agent audit entries, newest first (the "Agent activity" data), when the server runs.
    #[must_use]
    pub fn agent_activity(&self, filter: &AuditFilter) -> Vec<AuditRecord> {
        self.mcp
            .as_ref()
            .map(|m| m.audit().list(filter))
            .unwrap_or_default()
    }

    /// Undoes one audited agent write as a single undo step through the command queue.
    ///
    /// # Errors
    /// [`UndoError`]; `Failed` when the MCP server is not running.
    pub fn undo_agent_entry(&self, id: &str) -> Result<(), UndoError> {
        match &self.mcp {
            Some(m) => m.undo_audit_entry(id),
            None => Err(UndoError::Failed("the MCP server is not running".into())),
        }
    }

    /// Feeds a synthetic watcher event to the pump, as if the OS watcher had reported it.
    /// Tests use it to replay platform-specific event sequences (for example the
    /// Remove + Create pairs of Windows renames); [`Session::reindex`] doubles as a barrier,
    /// because jobs run in order.
    #[doc(hidden)]
    pub fn inject_watch_event(&self, ev: bitacora_watch::WatchEvent) {
        let _ = self.jobs.send(Job::Watch(ev));
    }

    /// Whether the watcher fell back to polling (`None` without a watcher).
    #[must_use]
    pub fn watcher_is_polling(&self) -> Option<bool> {
        self.watcher.as_ref().map(GraphWatcher::is_polling)
    }

    /// Status of the background sync engine.
    #[must_use]
    pub fn sync_status(&self) -> Option<SyncStatus> {
        lock(&self.sync).as_ref().map(|h| h.status())
    }

    /// The sync status stream: latest [`SyncStatusView`] (message, retry flag, backend, ahead and
    /// behind, conflict pages), updated on every engine state change. `None` without a
    /// background engine.
    #[must_use]
    pub fn sync_watch(&self) -> Option<SyncWatch> {
        self.sync_watch.clone()
    }

    /// Latest status view of the background engine.
    #[must_use]
    pub fn sync_view(&self) -> Option<SyncStatusView> {
        self.sync_watch.as_ref().map(|w| w.current().1)
    }

    /// What startup recovery found when the session opened (`None` without a background
    /// engine).
    #[must_use]
    pub fn recovery_report(&self) -> Option<&RecoveryReport> {
        self.recovery.as_ref()
    }

    fn engine(&self) -> Option<Arc<EngineHandle>> {
        lock(&self.sync).clone()
    }

    /// Resolves one conflict of the pending merge (blocks until applied through the command
    /// queue; the last resolution commits and pushes the merge).
    ///
    /// # Errors
    /// [`ResolveError`], including `Failed` when no engine runs.
    pub fn resolve_conflict(
        &self,
        id: &str,
        resolution: Resolution,
    ) -> Result<ResolveOutcome, ResolveError> {
        match self.engine() {
            Some(h) => h.resolve_conflict(id, resolution),
            None => Err(ResolveError::Failed("sync is not running".into())),
        }
    }

    /// Resolves every open conflict of the page at `path` (graph-relative) with one choice.
    ///
    /// # Errors
    /// As [`Session::resolve_conflict`].
    pub fn resolve_conflict_page(
        &self,
        path: &str,
        resolution: Resolution,
    ) -> Result<ResolveOutcome, ResolveError> {
        match self.engine() {
            Some(h) => h.resolve_page(path, resolution),
            None => Err(ResolveError::Failed("sync is not running".into())),
        }
    }

    /// Abandons a merge or rebase another tool left in progress (the "abort" choice).
    ///
    /// # Errors
    /// A message when no engine runs or the operation cannot be aborted.
    pub fn abort_external_operation(&self) -> Result<(), String> {
        match self.engine() {
            Some(h) => h.abort_external_operation(),
            None => Err("sync is not running".into()),
        }
    }

    fn read_backend(&self) -> Result<bitacora_sync::GixBackend, RuntimeError> {
        bitacora_sync::GixBackend::open(&self.root)
            .map_err(|e| RuntimeError::History(e.to_string()))
    }

    /// The commits that touched the page at `rel` (graph-relative), newest first, following
    /// renames; at most `limit`.
    ///
    /// # Errors
    /// [`RuntimeError::History`] when the graph is not a repository or reading it fails.
    pub fn page_history(&self, rel: &str, limit: usize) -> Result<Vec<HistoryEntry>, RuntimeError> {
        use bitacora_sync::GitBackend;
        let backend = self.read_backend()?;
        let Some(tip) = backend
            .resolve_ref("HEAD")
            .map_err(|e| RuntimeError::History(e.to_string()))?
        else {
            return Ok(Vec::new());
        };
        page_history(&backend, &tip, rel, limit).map_err(|e| RuntimeError::History(e.to_string()))
    }

    /// The text of the page at a history entry.
    ///
    /// # Errors
    /// [`RuntimeError::History`].
    pub fn history_version(&self, entry: &HistoryEntry) -> Result<String, RuntimeError> {
        let backend = self.read_backend()?;
        version_text(&backend, entry)
            .map_err(|e| RuntimeError::History(e.to_string()))?
            .ok_or_else(|| RuntimeError::History("that version is not readable text".into()))
    }

    /// Block-level diff between a historical version (`old`) and the page as it is on disk now
    /// (`new`). Flushes pending edits first so the comparison is against what the user sees.
    ///
    /// # Errors
    /// [`RuntimeError`] when the history or the page cannot be read.
    pub fn history_diff(&self, rel: &str, entry: &HistoryEntry) -> Result<PageDiff, RuntimeError> {
        let version = self.history_version(entry)?;
        self.queue.flush(Source::Ui)?;
        let path = GraphPath::new(rel).map_err(|_| RuntimeError::BadPath(rel.to_owned()))?;
        let current = std::fs::read_to_string(path.to_fs_path(&self.root)).map_err(|source| {
            RuntimeError::Read {
                path: rel.to_owned(),
                source,
            }
        })?;
        Ok(diff_version(&version, &current))
    }

    /// Restores the selected block differences of `selected` (taken from
    /// [`Session::history_diff`]) as ordinary core transactions on the page at `rel`. Undo with
    /// [`Session::undo_restore`].
    ///
    /// # Errors
    /// [`RuntimeError::Restore`] and queue errors.
    pub fn restore_blocks(
        &self,
        rel: &str,
        entry: &HistoryEntry,
        selected: &[BlockDiff],
    ) -> Result<RestoreReport, RuntimeError> {
        self.restore_with(rel, entry, &Selection::Blocks(selected))
    }

    /// Restores the whole page at `rel` to the version of `entry`, block by block, so untouched
    /// blocks are never rewritten. Undo with [`Session::undo_restore`].
    ///
    /// # Errors
    /// [`RuntimeError::Restore`] and queue errors.
    pub fn restore_page(
        &self,
        rel: &str,
        entry: &HistoryEntry,
    ) -> Result<RestoreReport, RuntimeError> {
        self.restore_with(rel, entry, &Selection::Whole)
    }

    /// Reverts a restore.
    ///
    /// # Errors
    /// [`RuntimeError::Restore`].
    pub fn undo_restore(&self, report: &RestoreReport) -> Result<(), RuntimeError> {
        report.undo(&self.queue)
    }

    fn restore_with(
        &self,
        rel: &str,
        entry: &HistoryEntry,
        selection: &Selection<'_>,
    ) -> Result<RestoreReport, RuntimeError> {
        let version = self.history_version(entry)?;
        // Make the file match the in-memory page, then (re)load it so core and disk agree.
        self.queue.flush(Source::Ui)?;
        let key = self.open_page(rel)?;
        restore(&self.queue, &self.root, &key, rel, &version, selection)
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
        let (mut engine, _watch) = build_engine(
            &self.root,
            &self.queue,
            opts,
            &current(&self.live_config),
            &self.index,
        )?;
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
        let title = derive_title(rel, None, &current(&self.live_config));
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

        // 0. Semantic sync and Pando: they read the index and core, so they stop before them.
        // Pending semantic operations stay in the ledger for the next session.
        if let Some(mut w) = self.semantic.take() {
            w.stop();
        }
        if let Some(mut p) = self.pando.take() {
            p.stop(left().min(Duration::from_secs(3)));
        }

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

/// Gives Pando agents least-privilege access to this graph's MCP server (ADR-031): the dedicated
/// `pando` token (Read scope; Write only when the graph's consent grants `agent_writes`), the
/// consent exclusions applied to everything that token reads, and the endpoint to register.
/// Does nothing unless the integration is active, consented and the MCP bridge is on.
fn provision_pando_mcp(server: &McpServer, p: &mut bitacora_pando::PandoOptions) {
    use bitacora_config::PandoFeature;
    use bitacora_mcp::{PANDO_TOKEN_NAME, ReadExclusions, Scope};

    let key = p.graph_key();
    if !p.settings.feature_enabled(PandoFeature::McpBridge) || !p.settings.has_consent(&key) {
        return;
    }
    let consent = p.settings.consent(&key);
    let scopes: &[Scope] = if consent.agent_writes {
        &[Scope::Read, Scope::Write]
    } else {
        &[Scope::Read]
    };
    match server.tokens().ensure_token(PANDO_TOKEN_NAME, scopes) {
        Ok(secret) => {
            server.set_read_exclusions(
                PANDO_TOKEN_NAME,
                Some(ReadExclusions::new(&consent.exclusions)),
            );
            p.mcp = Some(bitacora_pando::McpAccess::new(server.endpoint(), secret));
        }
        // Without the token agents simply get no Bitacora tools; the open never fails.
        Err(e) => tracing::warn!("cannot provision the pando MCP token: {e}"),
    }
}

/// The real managed-mode supervisor (cache instance dir, `pando` from `PATH` or the configured
/// binary); `None` leaves managed mode unavailable when no cache directory exists.
fn default_supervisor(
    settings: &bitacora_config::PandoSettings,
) -> Option<std::sync::Arc<dyn bitacora_pando::Supervisor>> {
    match bitacora_pando::ManagedSupervisor::with_defaults(settings.binary.as_deref()) {
        Ok(s) => Some(std::sync::Arc::new(s)),
        Err(e) => {
            tracing::warn!("managed Pando is unavailable: {e}");
            None
        }
    }
}
