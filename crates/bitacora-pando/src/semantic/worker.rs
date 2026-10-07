//! The semantic sync worker (BIT-US-0143).
//!
//! Two halves share the [`Ledger`]:
//!
//! * a **diff thread** that follows `IndexWriter` events (and runs the cold-start reconcile). It
//!   re-reads the affected file through the [`DocSource`], compares content hashes with what the
//!   ledger believes and enqueues upserts and deletes in the outbox;
//! * an async **sender** (a task on the Pando service runtime) that drains the outbox with a
//!   debounce, bounded concurrency and exponential backoff, and records every acknowledged
//!   operation in the ledger.
//!
//! The outbox stores no payload: the sender re-materialises the document from the index right
//! before sending, so it always ships the freshest eligible content and a block that became
//! ineligible meanwhile is deleted instead. Nothing here talks to Pando except
//! `KbClient::upsert` / `KbClient::delete`.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use bitacora_index::IndexEvent;
use pando::Error as PandoError;
use pando::kb::{KbClient, UpsertDocument};
use tokio::sync::{Notify, Semaphore, watch};
use tokio::task::JoinSet;

use super::SemanticError;
use super::doc::ContentPolicy;
use super::doc::SemanticDoc;
use super::ledger::{Ledger, Op, OutboxEntry, StateEntry};
use super::source::{DocSource, SharedPolicy};
use crate::activity::{ActivityEntry, ActivityKind};
use crate::events::{EventSink, PandoEvent, SyncProgress};
use crate::service::PandoService;

/// Provides the KB client once the endpoint is resolved.
pub type KbProvider = Arc<dyn Fn() -> Option<KbClient> + Send + Sync>;
/// Says whether documents may be sent right now (connected, consented).
pub type Gate = Arc<dyn Fn() -> bool + Send + Sync>;

/// Tuning of the sender.
#[derive(Debug, Clone)]
pub struct SenderConfig {
    /// Quiet time after a wake-up before sending, so a burst of edits becomes one upsert per block.
    pub debounce: Duration,
    /// Quiet time while collecting index events before diffing the touched files.
    pub coalesce: Duration,
    /// Maximum requests in flight.
    pub concurrency: usize,
    /// Outbox rows read per round.
    pub batch: usize,
    /// First retry delay; doubles per attempt.
    pub backoff_base: Duration,
    /// Longest retry delay (also used for permanent-looking errors).
    pub backoff_max: Duration,
    /// How often an idle sender looks at the outbox and the gate.
    pub idle_poll: Duration,
}

impl Default for SenderConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(1500),
            coalesce: Duration::from_millis(300),
            concurrency: 4,
            batch: 64,
            backoff_base: Duration::from_secs(2),
            backoff_max: Duration::from_secs(300),
            idle_poll: Duration::from_secs(5),
        }
    }
}

/// Diffs the index against the ledger into the outbox. Synchronous and free of any network.
#[derive(Clone)]
pub struct Reconciler {
    ledger: Arc<Ledger>,
    source: Arc<dyn DocSource>,
}

impl std::fmt::Debug for Reconciler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reconciler")
    }
}

impl Reconciler {
    /// A reconciler over `source` and `ledger`.
    #[must_use]
    pub fn new(ledger: Arc<Ledger>, source: Arc<dyn DocSource>) -> Self {
        Self { ledger, source }
    }

    /// Enqueues what changed in the file at `path`; returns the number of outbox changes.
    ///
    /// # Errors
    /// Reading the index or writing the ledger failed.
    pub fn sync_file(&self, path: &str) -> Result<usize, SemanticError> {
        let docs = self.source.file_docs(path)?;
        let tracked = self.ledger.tracked_for_path(path)?;
        let mut changed = 0;
        let mut present = BTreeSet::new();
        for d in &docs {
            present.insert(d.doc_id.as_str());
            let up_to_date = tracked
                .get(&d.doc_id)
                .is_some_and(|(_, t)| t.as_deref() == Some(d.content_hash.as_str()));
            if !up_to_date
                && self.ledger.enqueue_upsert(
                    &d.doc_id,
                    &d.block_uuid,
                    &d.file_path,
                    &d.content_hash,
                )?
            {
                changed += 1;
            }
        }
        for (doc_id, (uuid, t)) in &tracked {
            if present.contains(doc_id.as_str()) || t.is_none() {
                continue;
            }
            // Gone from this file. It may have moved to another one (still eligible): then the
            // upsert for the new location replaces the document instead of deleting it.
            match self.source.block_doc(uuid)? {
                Some(d) if d.file_path != path => {
                    if self.ledger.enqueue_upsert(
                        &d.doc_id,
                        &d.block_uuid,
                        &d.file_path,
                        &d.content_hash,
                    )? {
                        changed += 1;
                    }
                }
                Some(_) => {}
                None => {
                    if self.ledger.enqueue_delete(doc_id, uuid, path)? {
                        changed += 1;
                    }
                }
            }
        }
        Ok(changed)
    }

    /// Compares every eligible document with the ledger (cold start, `BulkFinished`, policy
    /// change). Returns the number of outbox changes.
    ///
    /// An index with no files at all never deletes anything: it is far more likely to be
    /// rebuilding than to describe an emptied graph.
    ///
    /// # Errors
    /// Reading the index or writing the ledger failed.
    pub fn reconcile_all(&self) -> Result<usize, SemanticError> {
        let mut paths: BTreeSet<String> = self.source.file_paths()?.into_iter().collect();
        let known = self.ledger.known_paths()?;
        if paths.is_empty() {
            tracing::warn!("semantic reconcile skipped: the index lists no files");
            return Ok(0);
        }
        paths.extend(known);
        let mut changed = 0;
        for p in &paths {
            changed += self.sync_file(p)?;
        }
        Ok(changed)
    }

    /// Applies one index event; returns the number of outbox changes.
    ///
    /// # Errors
    /// Reading the index or writing the ledger failed.
    pub fn handle_event(&self, ev: &IndexEvent) -> Result<usize, SemanticError> {
        match ev {
            IndexEvent::BulkFinished => self.reconcile_all(),
            other => {
                let mut changed = 0;
                for p in event_paths(other) {
                    changed += self.sync_file(&p)?;
                }
                Ok(changed)
            }
        }
    }
}

fn event_paths(ev: &IndexEvent) -> Vec<String> {
    match ev {
        IndexEvent::FileReplaced { path, .. } | IndexEvent::FileDeleted { path, .. } => {
            vec![path.clone()]
        }
        IndexEvent::FileRenamed { from, to } => vec![from.clone(), to.clone()],
        IndexEvent::BulkFinished => Vec::new(),
    }
}

/// Snapshot of the worker for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStatus {
    /// Documents acknowledged by the server.
    pub synced: u64,
    /// Operations waiting in the outbox.
    pub pending: u64,
    /// Error of the latest failed send still waiting for a retry.
    pub last_error: Option<String>,
}

/// What [`SemanticWorker::start`] needs.
pub struct SemanticParams {
    /// Eligible documents.
    pub source: Arc<dyn DocSource>,
    /// The sync ledger.
    pub ledger: Arc<Ledger>,
    /// The policy `source` applies; [`SemanticWorker::set_policy`] replaces its value.
    pub policy: SharedPolicy,
    /// Index events (`IndexWriter::subscribe` / `Indexer::subscribe`), subscribed before the
    /// first reconcile so no change is missed.
    pub events: Receiver<IndexEvent>,
    /// KB client provider.
    pub kb: KbProvider,
    /// Send gate.
    pub gate: Gate,
    /// Where progress events go.
    pub sink: Option<EventSink>,
    /// Sender tuning.
    pub config: SenderConfig,
}

impl std::fmt::Debug for SemanticParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticParams")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

enum Cmd {
    Reconcile,
    Purge,
}

/// A running semantic sync (diff thread plus async sender).
pub struct SemanticWorker {
    ledger: Arc<Ledger>,
    policy: SharedPolicy,
    cmd: Sender<Cmd>,
    notify: Arc<Notify>,
    stop_flag: Arc<AtomicBool>,
    stop_tx: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for SemanticWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticWorker")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl SemanticWorker {
    /// Starts the diff thread (which first runs the cold-start reconcile) and spawns the sender on
    /// `handle`.
    ///
    /// # Errors
    /// [`SemanticError::Worker`] when the thread cannot be spawned.
    pub fn start(
        handle: &tokio::runtime::Handle,
        params: SemanticParams,
    ) -> Result<Self, SemanticError> {
        let notify = Arc::new(Notify::new());
        let stop_flag = Arc::new(AtomicBool::new(false));
        let (stop_tx, stop_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let reconciler = Reconciler::new(Arc::clone(&params.ledger), Arc::clone(&params.source));

        let thread = {
            let notify = Arc::clone(&notify);
            let stop = Arc::clone(&stop_flag);
            let ledger = Arc::clone(&params.ledger);
            let coalesce = params.config.coalesce;
            let events = params.events;
            std::thread::Builder::new()
                .name("bitacora-semantic-diff".into())
                .spawn(move || {
                    diff_loop(
                        &reconciler,
                        &ledger,
                        &events,
                        &cmd_rx,
                        &notify,
                        &stop,
                        coalesce,
                    )
                })
                .map_err(|e| SemanticError::Worker(e.to_string()))?
        };
        let sender = OutboxSender {
            ledger: Arc::clone(&params.ledger),
            source: params.source,
            kb: params.kb,
            gate: params.gate,
            sink: params.sink,
            config: params.config,
            notify: Arc::clone(&notify),
            offline_rounds: std::sync::atomic::AtomicU32::new(0),
        };
        let task = handle.spawn(sender.run(stop_rx));
        Ok(Self {
            ledger: params.ledger,
            policy: params.policy,
            cmd: cmd_tx,
            notify,
            stop_flag,
            stop_tx,
            thread: Some(thread),
            task: Some(task),
        })
    }

    /// Replaces the content policy and re-diffs the graph: documents that are no longer eligible
    /// (new exclusions, private pages) are deleted from Pando, newly eligible ones are sent.
    pub fn set_policy(&self, policy: ContentPolicy) {
        *self.policy.write() = policy;
        self.reconcile();
    }

    /// The content policy the worker and the searchers share.
    #[must_use]
    pub fn policy(&self) -> SharedPolicy {
        Arc::clone(&self.policy)
    }

    /// Re-diffs the whole graph (after the exclusions changed, for instance).
    pub fn reconcile(&self) {
        let _ = self.cmd.send(Cmd::Reconcile);
    }

    /// Schedules the removal of every document of this graph from Pando (consent revoked, "remove
    /// my data"). Documents are found in the ledger, never by listing the server.
    pub fn purge(&self) {
        let _ = self.cmd.send(Cmd::Purge);
    }

    /// Sends everything pending again now, ignoring backoff.
    pub fn retry_now(&self) {
        let _ = self.ledger.retry_now();
        self.notify.notify_one();
    }

    /// Counts for the UI.
    #[must_use]
    pub fn status(&self) -> SemanticStatus {
        SemanticStatus {
            synced: self.ledger.synced_count().unwrap_or(0),
            pending: self.ledger.pending_count().unwrap_or(0),
            last_error: self.ledger.last_error().unwrap_or(None),
        }
    }

    /// The ledger (diagnostics).
    #[must_use]
    pub fn ledger(&self) -> &Arc<Ledger> {
        &self.ledger
    }

    /// Stops both halves. Pending outbox rows stay in the ledger for the next session.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.stop_tx.send(true);
        if let Some(t) = self.task.take() {
            t.abort();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SemanticWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn diff_loop(
    rec: &Reconciler,
    ledger: &Ledger,
    events: &Receiver<IndexEvent>,
    cmds: &Receiver<Cmd>,
    notify: &Notify,
    stop: &AtomicBool,
    coalesce: Duration,
) {
    let run = |what: &str, r: Result<usize, SemanticError>| match r {
        Ok(n) if n > 0 => notify.notify_one(),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "semantic {what} failed"),
    };
    run("cold-start reconcile", rec.reconcile_all());
    let poll = Duration::from_millis(100).min(coalesce.max(Duration::from_millis(5)));
    while !stop.load(Ordering::SeqCst) {
        while let Ok(c) = cmds.try_recv() {
            match c {
                Cmd::Reconcile => run("reconcile", rec.reconcile_all()),
                Cmd::Purge => match ledger.purge_all() {
                    Ok(_) => notify.notify_one(),
                    Err(e) => tracing::warn!(error = %e, "semantic purge failed"),
                },
            }
        }
        match events.recv_timeout(poll) {
            Ok(first) => {
                // Collect the burst, then diff each touched file once.
                let mut batch = vec![first];
                while let Ok(ev) = events.recv_timeout(coalesce) {
                    batch.push(ev);
                }
                if batch.iter().any(|e| matches!(e, IndexEvent::BulkFinished)) {
                    run("reconcile", rec.reconcile_all());
                    continue;
                }
                let paths: BTreeSet<String> = batch.iter().flat_map(event_paths).collect();
                for p in paths {
                    run("file diff", rec.sync_file(&p));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // The index went away; keep serving commands until stopped.
                std::thread::sleep(poll);
            }
        }
    }
}

struct OutboxSender {
    ledger: Arc<Ledger>,
    source: Arc<dyn DocSource>,
    kb: KbProvider,
    gate: Gate,
    sink: Option<EventSink>,
    config: SenderConfig,
    notify: Arc<Notify>,
    /// Consecutive rounds that ended offline (drives the global backoff).
    offline_rounds: std::sync::atomic::AtomicU32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The server is unreachable or not usable right now: stop the round.
    Offline,
    /// This operation failed; others may succeed.
    Transient,
    /// A client error that retrying will not fix; retried rarely.
    Permanent,
}

fn classify(e: &PandoError) -> Kind {
    match e {
        PandoError::Unreachable(_)
        | PandoError::Timeout
        | PandoError::NotConfigured(_)
        | PandoError::Unauthorized => Kind::Offline,
        PandoError::Server { status, .. }
            if (400..500).contains(status) && *status != 408 && *status != 429 =>
        {
            Kind::Permanent
        }
        PandoError::Config(_) => Kind::Permanent,
        _ => Kind::Transient,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl OutboxSender {
    fn backoff(&self, attempts: u32, kind: Kind) -> i64 {
        let max = self.config.backoff_max;
        let d = if kind == Kind::Permanent {
            max
        } else {
            self.config
                .backoff_base
                .saturating_mul(1u32.checked_shl(attempts.min(16)).unwrap_or(u32::MAX))
                .min(max)
        };
        now_ms().saturating_add(i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    async fn run(self, mut stop: watch::Receiver<bool>) {
        let this = Arc::new(self);
        loop {
            if *stop.borrow() {
                return;
            }
            let wait = match this.ledger.next_due_at() {
                Ok(Some(t)) => {
                    Duration::from_millis(u64::try_from((t - now_ms()).max(0)).unwrap_or(0))
                        .min(this.config.idle_poll)
                }
                _ => this.config.idle_poll,
            }
            .max(Duration::from_millis(20));
            let woken = tokio::select! {
                _ = stop.changed() => return,
                () = this.notify.notified() => true,
                () = tokio::time::sleep(wait) => false,
            };
            if woken {
                // Debounce: let a burst of enqueues settle.
                tokio::select! {
                    _ = stop.changed() => return,
                    () = tokio::time::sleep(this.config.debounce) => {}
                }
            }
            if !(this.gate)() {
                tokio::select! {
                    _ = stop.changed() => return,
                    () = tokio::time::sleep(this.config.idle_poll.min(Duration::from_millis(500))) => {}
                }
                continue;
            }
            let Some(kb) = (this.kb)() else { continue };
            Arc::clone(&this).drain(&kb, &stop).await;
        }
    }

    /// Sends every due row; stops early when the server is unreachable.
    async fn drain(self: Arc<Self>, kb: &KbClient, stop: &watch::Receiver<bool>) {
        let offline = Arc::new(AtomicBool::new(false));
        let sem = Arc::new(Semaphore::new(self.config.concurrency.max(1)));
        let mut done = 0u64;
        let mut total: Option<u64> = None;
        loop {
            if *stop.borrow() || offline.load(Ordering::SeqCst) {
                break;
            }
            let due = match self.ledger.due(now_ms(), self.config.batch.max(1)) {
                Ok(d) if !d.is_empty() => d,
                _ => break,
            };
            if total.is_none() {
                total = self.ledger.pending_count().ok();
                self.progress(done, total);
            }
            let mut set = JoinSet::new();
            for entry in due {
                if offline.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(permit) = Arc::clone(&sem).acquire_owned().await else {
                    break;
                };
                if offline.load(Ordering::SeqCst) {
                    break;
                }
                let this = Arc::clone(&self);
                let kb = kb.clone();
                let off = Arc::clone(&offline);
                set.spawn(async move {
                    let r = this.send_one(&kb, &entry, &off).await;
                    drop(permit);
                    (r, entry.op, entry.block_uuid)
                });
            }
            let mut upserted = Vec::new();
            let mut deleted = Vec::new();
            while let Some(r) = set.join_next().await {
                if let Ok((true, op, uuid)) = r {
                    done += 1;
                    match op {
                        Op::Upsert => upserted.push(uuid),
                        Op::Delete => deleted.push(uuid),
                    }
                    self.progress(done, total);
                }
            }
            self.batch_done("upserted", upserted);
            self.batch_done("deleted", deleted);
        }
        if offline.load(Ordering::SeqCst) {
            // Postpone the whole outbox, with growing delays while the outage lasts.
            let n = self.offline_rounds.fetch_add(1, Ordering::SeqCst);
            let _ = self.ledger.defer_all(self.backoff(n, Kind::Offline));
        } else if done > 0 {
            self.offline_rounds.store(0, Ordering::SeqCst);
        }
        if total.is_some() {
            self.progress(done, total);
        }
    }

    /// Logs an acknowledged batch by count and (capped) block uuids, never by content.
    fn batch_done(&self, what: &str, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        if let Some(s) = &self.sink {
            let n = u64::try_from(ids.len()).unwrap_or(u64::MAX);
            s.record(ActivityEntry::new(ActivityKind::Sync, what, n, ids));
        }
    }

    fn progress(&self, done: u64, total: Option<u64>) {
        if let Some(s) = &self.sink {
            s.emit(&PandoEvent::SyncProgress(SyncProgress { done, total }));
        }
    }

    /// Sends one outbox row; `true` when the server acknowledged it.
    async fn send_one(&self, kb: &KbClient, entry: &OutboxEntry, offline: &AtomicBool) -> bool {
        let outcome = match entry.op {
            Op::Delete => self.delete_remote(kb, entry).await.map(|()| None),
            Op::Upsert => {
                let source = Arc::clone(&self.source);
                let uuid = entry.block_uuid.clone();
                let doc = tokio::task::spawn_blocking(move || source.block_doc(&uuid)).await;
                match doc {
                    Ok(Ok(Some(d))) => upsert(kb, &d).await.map(|()| Some(d)),
                    // Gone or no longer eligible: make sure it is not left on the server.
                    Ok(Ok(None)) => self.delete_remote(kb, entry).await.map(|()| None),
                    Ok(Err(e)) => {
                        let _ = self.ledger.fail(
                            entry,
                            self.backoff(entry.attempts, Kind::Transient),
                            &e.to_string(),
                        );
                        return false;
                    }
                    Err(e) => {
                        let _ = self.ledger.fail(
                            entry,
                            self.backoff(entry.attempts, Kind::Transient),
                            &e.to_string(),
                        );
                        return false;
                    }
                }
            }
        };
        match outcome {
            Ok(sent) => {
                let state = sent.map(|d| StateEntry {
                    doc_id: d.doc_id,
                    block_uuid: d.block_uuid,
                    path: d.file_path,
                    content_hash: d.content_hash,
                });
                if let Err(e) = self.ledger.complete(entry, state.as_ref()) {
                    tracing::warn!(error = %e, "semantic ledger update failed");
                    return false;
                }
                true
            }
            Err(e) => {
                let kind = classify(&e);
                if kind == Kind::Offline {
                    offline.store(true, Ordering::SeqCst);
                }
                // Never log content or tokens: the error text is the SDK's redacted message.
                let _ = self
                    .ledger
                    .fail(entry, self.backoff(entry.attempts, kind), &e.to_string());
                false
            }
        }
    }

    async fn delete_remote(&self, kb: &KbClient, entry: &OutboxEntry) -> Result<(), PandoError> {
        match kb.delete(&entry.doc_id).await {
            // Already gone: that is the state we wanted.
            Err(PandoError::Server { status: 404, .. }) | Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

async fn upsert(kb: &KbClient, d: &SemanticDoc) -> Result<(), PandoError> {
    let mut req = UpsertDocument::new(d.doc_id.clone(), d.text.clone());
    req.metadata = Some(d.metadata.clone());
    req.tags = d.tags.clone();
    kb.upsert(&req).await.map(drop)
}

/// Starts the worker on `service`'s runtime, or returns `None` when the service is inactive
/// (disabled, no consent, mode off, invalid settings): nothing is read or sent then.
///
/// The send gate opens only while the service reports `Connected`.
///
/// # Errors
/// [`SemanticError::Worker`] when the diff thread cannot start.
pub fn attach(
    service: &PandoService,
    source: Arc<dyn DocSource>,
    policy: SharedPolicy,
    ledger: Arc<Ledger>,
    events: Receiver<IndexEvent>,
    config: SenderConfig,
) -> Result<Option<SemanticWorker>, SemanticError> {
    let Some(handle) = service.handle() else {
        return Ok(None);
    };
    let probe = service.probe();
    let kb_probe = probe.clone();
    let params = SemanticParams {
        source,
        policy,
        ledger,
        events,
        kb: Arc::new(move || kb_probe.client().map(|c| c.kb())),
        gate: Arc::new(move || probe.is_connected()),
        sink: Some(service.sink()),
        config,
    };
    SemanticWorker::start(&handle, params).map(Some)
}
