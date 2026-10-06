//! The single-writer command queue (BIT-US-0062, BIT-SP-0005.R1; ADR-011, ADR-012).
//!
//! One consumer thread owns the [`Workspace`] and the [`FileStore`]. UI, MCP, sync and the
//! watcher submit [`Request`]s tagged with a [`Source`]; they run strictly in submission order
//! and each gets its result through a [`Reply`] that works from threads (`wait`) and from async
//! code (`.await`) without tying core to an executor.
//!
//! Read access never goes through the writer: after every change the consumer publishes
//! immutable [`PageSnapshot`]s that any thread can fetch with [`CommandQueue::snapshot`].
//!
//! Seams for later stories:
//! * indexing / echo suppression: register an observer in [`QueueConfig`]; it receives
//!   [`QueueEvent`]s (committed transactions, written/deleted files, reloaded pages) on the
//!   consumer thread;
//! * the sync engine: [`CommandQueue::acquire`] flushes and then blocks every other writer until
//!   the [`QueueLock`] is dropped; [`QueueLock::apply`] writes file changes with an
//!   expected-content check. `bitacora-sync`'s `GraphWriter` adapter lives in the app/cli glue
//!   (sync depends on core, not the other way round) and maps `FileChange` to [`FileEdit`];
//! * debounced writes (BIT-US-0063): by default the consumer writes a dirty page 400 ms after its
//!   last edit (at most 2 s after the first unwritten one, see [`DebounceConfig`]); failures are
//!   retried with backoff, conflicts are parked and reported through [`QueueEvent`]s.
//!   [`Request::Flush`], [`CommandQueue::acquire`] and shutdown flush everything on demand.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::task::{Context, Poll, Waker};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use uuid::Uuid;

use crate::editor::TakeDisk;
use crate::editor::{
    BlockId, Cmd, CommitError, FileStore, FlushReport, Op, Transaction, TxId, Workspace,
    WrittenFile,
};
use crate::graph::PageKey;
use crate::graph_path::GraphPath;
use crate::write_queue::{DebounceConfig, WriteQueue};

/// Page written while a conflict is pending: which side wins.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Keep {
    /// Overwrite the external version with ours (it is backed up to `logseq/bak` first).
    Mine,
    /// Replace our unsaved edits with the external version (they are backed up first).
    Disk,
}

/// Who submitted a command (kept in the audit log; MCP writes are audited, rule 7).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// The desktop UI.
    Ui,
    /// An MCP client.
    Mcp,
    /// The sync engine.
    Sync,
    /// The file watcher (changes made by other programs).
    External,
}

/// What to do.
#[derive(Debug)]
pub enum Request {
    /// Plan and commit a command.
    Run {
        /// Transaction label.
        label: &'static str,
        /// The command.
        cmd: Cmd,
    },
    /// Commit ready-made ops (optimistic: `before` fields must match).
    Commit {
        /// Transaction label.
        label: &'static str,
        /// The ops.
        ops: Vec<Op>,
    },
    /// Load or reload a page from bytes (refused while the page has unsaved edits).
    LoadPage {
        /// Page key.
        key: PageKey,
        /// Title.
        title: String,
        /// File path.
        path: Option<GraphPath>,
        /// File content.
        bytes: Vec<u8>,
    },
    /// Write every dirty page now.
    Flush,
    /// Resolve a write conflict (BIT-US-0065/0066).
    Resolve {
        /// Conflicting page.
        key: PageKey,
        /// Which side wins.
        keep: Keep,
    },
    /// Files were reported deleted by the watcher: drop the pages without unsaved edits (dirty
    /// ones are recreated by the next flush).
    CheckMissing {
        /// Deleted files.
        paths: Vec<GraphPath>,
    },
}

/// Successful result of a [`Request`].
#[derive(Debug)]
pub enum Response {
    /// A transaction was committed.
    Committed(Transaction),
    /// A page was loaded.
    Loaded,
    /// Dirty pages were written.
    Flushed(FlushReport),
    /// A conflict was resolved ("keep mine" reports its write, "take disk" an empty report).
    Resolved(FlushReport, Option<TakeDisk>),
    /// Pages dropped because their file was deleted externally.
    Removed(Vec<PageKey>),
}

/// Why a request failed.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// The command was refused, an op failed or an invariant broke (nothing changed).
    #[error(transparent)]
    Commit(#[from] CommitError),
    /// The queue thread is gone.
    #[error("command queue is closed")]
    Closed,
    /// The writer could not be acquired in time or pages could not be flushed.
    #[error("graph writer is busy")]
    Busy,
    /// A page has unsaved edits, so it was not reloaded.
    #[error("page {0:?} has unsaved edits")]
    PageDirty(PageKey),
    /// A file changed on disk since the caller read it; nothing was applied.
    #[error("`{0}` changed on disk since it was read")]
    Stale(String),
    /// Invalid input (e.g. a bad path).
    #[error("invalid request: {0}")]
    Invalid(String),
    /// I/O failure.
    #[error("store error: {0}")]
    Store(String),
}

// ---- oneshot reply -------------------------------------------------------------------------

struct Slot<T> {
    value: Option<T>,
    closed: bool,
    waker: Option<Waker>,
}

struct Shared<T> {
    slot: Mutex<Slot<T>>,
    cv: Condvar,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sending half of a one-shot channel.
pub struct ReplySender<T>(Arc<Shared<T>>);

/// Receiving half of a one-shot channel: block with [`Reply::wait`] or `.await` it.
pub struct Reply<T>(Arc<Shared<T>>);

/// Creates a one-shot channel.
#[must_use]
pub fn oneshot<T>() -> (ReplySender<T>, Reply<T>) {
    let s = Arc::new(Shared {
        slot: Mutex::new(Slot {
            value: None,
            closed: false,
            waker: None,
        }),
        cv: Condvar::new(),
    });
    (ReplySender(s.clone()), Reply(s))
}

impl<T> ReplySender<T> {
    /// Delivers the value.
    pub fn send(self, value: T) {
        let mut g = lock(&self.0.slot);
        g.value = Some(value);
        let w = g.waker.take();
        drop(g);
        self.0.cv.notify_all();
        if let Some(w) = w {
            w.wake();
        }
    }
}

impl<T> Drop for ReplySender<T> {
    fn drop(&mut self) {
        let mut g = lock(&self.0.slot);
        g.closed = true;
        let w = g.waker.take();
        drop(g);
        self.0.cv.notify_all();
        if let Some(w) = w {
            w.wake();
        }
    }
}

impl<T> Reply<T> {
    /// Blocks until the value arrives; `None` when the sender was dropped without sending.
    #[must_use]
    pub fn wait(self) -> Option<T> {
        let mut g = lock(&self.0.slot);
        loop {
            if let Some(v) = g.value.take() {
                return Some(v);
            }
            if g.closed {
                return None;
            }
            g = self
                .0
                .cv
                .wait(g)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Like [`Reply::wait`] with a timeout. `Err(self)` on timeout so the caller can retry.
    ///
    /// # Errors
    /// Gives the reply back when the timeout elapsed.
    pub fn wait_timeout(self, timeout: Duration) -> Result<Option<T>, Self> {
        let deadline = std::time::Instant::now() + timeout;
        let mut g = lock(&self.0.slot);
        loop {
            if let Some(v) = g.value.take() {
                return Ok(Some(v));
            }
            if g.closed {
                return Ok(None);
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                drop(g);
                return Err(self);
            }
            g = self
                .0
                .cv
                .wait_timeout(g, deadline - now)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// Non-blocking poll.
    #[must_use]
    pub fn try_take(&self) -> Option<T> {
        lock(&self.0.slot).value.take()
    }
}

impl<T> Future for Reply<T> {
    type Output = Option<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut g = lock(&self.0.slot);
        if let Some(v) = g.value.take() {
            return Poll::Ready(Some(v));
        }
        if g.closed {
            return Poll::Ready(None);
        }
        g.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

// ---- read snapshots ------------------------------------------------------------------------

/// A block in a [`PageSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotBlock {
    /// Block id.
    pub id: BlockId,
    /// Parent block.
    pub parent: Option<BlockId>,
    /// 1-based depth.
    pub depth: usize,
    /// Block text.
    pub text: String,
    /// `id::` uuid, when present.
    pub uuid: Option<Uuid>,
}

/// Immutable copy of a page for views and MCP reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSnapshot {
    /// Page key.
    pub key: PageKey,
    /// Title.
    pub title: String,
    /// File path.
    pub path: Option<GraphPath>,
    /// Pre-block text.
    pub preamble: Option<String>,
    /// Blocks in document order.
    pub blocks: Vec<SnapshotBlock>,
    /// Unsaved edits exist.
    pub dirty: bool,
    /// Sequence number of the last change that produced this snapshot.
    pub version: u64,
}

type SnapshotMap = Arc<RwLock<HashMap<PageKey, Arc<PageSnapshot>>>>;

fn snapshot_of(ws: &Workspace, key: &PageKey, version: u64) -> Option<PageSnapshot> {
    let p = ws.page(key)?;
    let blocks = p
        .dfs()
        .into_iter()
        .filter_map(|id| {
            let b = p.block(id)?;
            Some(SnapshotBlock {
                id,
                parent: b.parent,
                depth: p.depth_of(id),
                text: b.text.clone(),
                uuid: b.uuid,
            })
        })
        .collect();
    Some(PageSnapshot {
        key: key.clone(),
        title: p.title.clone(),
        path: p.path.clone(),
        preamble: p.preamble.clone(),
        blocks,
        dirty: p.needs_write(),
        version,
    })
}

// ---- audit and events ----------------------------------------------------------------------

/// One applied mutation (the audit trail required for MCP writes, rule 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// Monotonic sequence number.
    pub seq: u64,
    /// Who asked.
    pub source: Source,
    /// Transaction label or file-edit description.
    pub label: String,
    /// Transaction, for undo (`None` for file edits).
    pub tx: Option<TxId>,
    /// Touched pages.
    pub pages: Vec<PageKey>,
    /// When.
    pub at: SystemTime,
}

/// Notifications emitted on the consumer thread after each change.
#[derive(Debug, Clone)]
pub enum QueueEvent {
    /// A transaction was committed.
    Committed {
        /// Audit sequence number.
        seq: u64,
        /// Who asked.
        source: Source,
        /// The transaction id.
        tx: TxId,
        /// Touched pages.
        pages: Vec<PageKey>,
    },
    /// Files were written or removed by a flush (index them, remember their hashes for echo
    /// suppression).
    Flushed(FlushReport),
    /// Files were written or removed by [`QueueLock::apply`].
    FilesApplied {
        /// Written files.
        written: Vec<WrittenFile>,
        /// Removed files.
        deleted: Vec<GraphPath>,
    },
    /// A loaded page was replaced by new disk content (ids changed).
    PageReloaded(PageKey),
    /// Pages were not written because their file changed on disk (nothing was overwritten;
    /// resolve with [`Request::Resolve`] or merge the external change).
    Conflict(Vec<PageKey>),
    /// Writes failed; the pages stay dirty and are retried with backoff. Drives the persistent
    /// "cannot save" notice.
    WriteFailed {
        /// Pages and error messages.
        failed: Vec<(PageKey, String)>,
        /// Files not written.
        unwritten: Vec<GraphPath>,
    },
    /// Files deleted externally while their page had unsaved edits were recreated.
    Recreated(Vec<PageKey>),
    /// Pages removed because their file was deleted externally (remove from the index).
    PagesRemoved(Vec<PageKey>),
}

/// Observer of [`QueueEvent`]s. Runs on the consumer thread: keep it quick.
pub type Observer = Box<dyn Fn(&QueueEvent) + Send>;

/// Queue configuration.
pub struct QueueConfig {
    /// Called for every event.
    pub observers: Vec<Observer>,
    /// Write dirty pages after every committed command (default `false`; mainly for tests).
    pub auto_flush: bool,
    /// Debounced background writes (default 400 ms / 2 s, BIT-US-0063). `None` writes only on
    /// [`Request::Flush`], [`CommandQueue::acquire`] and shutdown.
    pub debounce: Option<DebounceConfig>,
    /// Entries kept in the audit ring.
    pub audit_capacity: usize,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            observers: Vec::new(),
            auto_flush: false,
            debounce: Some(DebounceConfig::default()),
            audit_capacity: 10_000,
        }
    }
}

// ---- file edits for sync -------------------------------------------------------------------

/// One work-tree change requested by the sync engine (mirrors `bitacora_sync::FileChange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEdit {
    /// Create or replace a file.
    Write {
        /// Graph-relative path.
        path: String,
        /// New content.
        content: Vec<u8>,
        /// Content believed to be on disk; `None` = the file must not exist.
        expected: Option<Vec<u8>>,
    },
    /// Remove a file.
    Delete {
        /// Graph-relative path.
        path: String,
        /// Content believed to be on disk; `None` = the file must not exist (then this is a
        /// no-op, and an existing file is refused as stale).
        expected: Option<Vec<u8>>,
    },
}

enum LockMsg {
    Apply(Vec<FileEdit>, ReplySender<Result<(), QueueError>>),
    Release,
}

/// Exclusive access to the graph: every other command waits while this is alive.
pub struct QueueLock {
    tx: Sender<LockMsg>,
}

impl QueueLock {
    /// Applies `edits` all-or-nothing: every `expected` is checked first and nothing is written
    /// when one differs ([`QueueError::Stale`]). Loaded pages for the touched files are reloaded
    /// or dropped.
    ///
    /// # Errors
    /// [`QueueError::Stale`], [`QueueError::Invalid`], [`QueueError::Store`] or
    /// [`QueueError::Closed`].
    pub fn apply(&mut self, edits: Vec<FileEdit>) -> Result<(), QueueError> {
        let (s, r) = oneshot();
        self.tx
            .send(LockMsg::Apply(edits, s))
            .map_err(|_| QueueError::Closed)?;
        r.wait().unwrap_or(Err(QueueError::Closed))
    }
}

impl Drop for QueueLock {
    fn drop(&mut self) {
        let _ = self.tx.send(LockMsg::Release);
    }
}

// ---- the queue -----------------------------------------------------------------------------

macro_rules! opaque_debug {
    ($($t:ty => $name:literal),* $(,)?) => {$(
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str($name)
            }
        }
    )*};
}

opaque_debug! {
    QueueConfig => "QueueConfig",
    QueueLock => "QueueLock",
    CommandQueue => "CommandQueue",
    QueueJoin => "QueueJoin",
}

impl<T> std::fmt::Debug for ReplySender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReplySender")
    }
}

impl<T> std::fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reply")
    }
}

type CommandReply = ReplySender<Result<Response, QueueError>>;

struct Command {
    source: Source,
    request: Request,
    reply: CommandReply,
}

struct AcquireMsg {
    ready: ReplySender<Result<(), QueueError>>,
    locks: Receiver<LockMsg>,
    cancelled: Arc<AtomicBool>,
}

enum Msg {
    Command(Command),
    Acquire(AcquireMsg),
    Shutdown,
}

struct Inner {
    audit: Mutex<VecDeque<AuditEntry>>,
    snapshots: SnapshotMap,
}

/// Producer handle: cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct CommandQueue {
    tx: Sender<Msg>,
    inner: Arc<Inner>,
}

/// Owner of the consumer thread.
pub struct QueueJoin {
    handle: Option<JoinHandle<(Workspace, FlushReport)>>,
    tx: Sender<Msg>,
}

impl QueueJoin {
    /// Flushes every dirty page, stops the consumer after the commands already queued and
    /// returns the workspace.
    ///
    /// # Errors
    /// [`QueueError::Closed`] when the thread panicked.
    pub fn shutdown(self) -> Result<Workspace, QueueError> {
        self.shutdown_with_report().map(|(ws, _)| ws)
    }

    /// Like [`QueueJoin::shutdown`] and also returns the report of the final flush: pages left
    /// in `conflicts` / `failed` still hold unsaved edits (warn the user before quitting).
    ///
    /// # Errors
    /// [`QueueError::Closed`] when the thread panicked.
    pub fn shutdown_with_report(mut self) -> Result<(Workspace, FlushReport), QueueError> {
        let _ = self.tx.send(Msg::Shutdown);
        match self.handle.take() {
            Some(h) => h.join().map_err(|_| QueueError::Closed),
            None => Err(QueueError::Closed),
        }
    }
}

impl CommandQueue {
    /// Starts the consumer thread, which takes ownership of `ws` and `store`.
    ///
    /// # Panics
    /// When the OS refuses to spawn a thread.
    #[must_use]
    pub fn spawn(
        ws: Workspace,
        store: Box<dyn FileStore + Send>,
        config: QueueConfig,
    ) -> (Self, QueueJoin) {
        let (tx, rx) = mpsc::channel();
        let inner = Arc::new(Inner {
            audit: Mutex::new(VecDeque::new()),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        });
        let sched = config.debounce.map(WriteQueue::new);
        let worker = Worker {
            ws,
            store,
            config,
            inner: inner.clone(),
            seq: 0,
            sched,
            started: Instant::now(),
        };
        worker.publish_all();
        #[allow(clippy::expect_used)] // thread spawn failure is unrecoverable at start-up
        let handle = std::thread::Builder::new()
            .name("bitacora-writer".into())
            .spawn(move || worker.run(&rx))
            .expect("spawn writer thread");
        (
            Self {
                tx: tx.clone(),
                inner,
            },
            QueueJoin {
                handle: Some(handle),
                tx,
            },
        )
    }

    /// Queues `request`; the result arrives through the returned [`Reply`].
    #[must_use]
    pub fn submit(&self, source: Source, request: Request) -> Reply<Result<Response, QueueError>> {
        let (reply, rx) = oneshot();
        // If the thread is gone the sender is dropped with the message and `wait` yields None.
        let _ = self.tx.send(Msg::Command(Command {
            source,
            request,
            reply,
        }));
        rx
    }

    /// Submits and waits.
    ///
    /// # Errors
    /// Whatever the request fails with; [`QueueError::Closed`] when the queue stopped.
    pub fn execute(&self, source: Source, request: Request) -> Result<Response, QueueError> {
        self.submit(source, request)
            .wait()
            .unwrap_or(Err(QueueError::Closed))
    }

    /// Plans and commits `cmd`, waiting for the transaction.
    ///
    /// # Errors
    /// As [`CommandQueue::execute`].
    pub fn run(
        &self,
        source: Source,
        label: &'static str,
        cmd: Cmd,
    ) -> Result<Transaction, QueueError> {
        match self.execute(source, Request::Run { label, cmd })? {
            Response::Committed(tx) => Ok(tx),
            _ => Err(QueueError::Invalid("unexpected response".into())),
        }
    }

    /// Flushes dirty pages, waiting for the report.
    ///
    /// # Errors
    /// As [`CommandQueue::execute`].
    pub fn flush(&self, source: Source) -> Result<FlushReport, QueueError> {
        match self.execute(source, Request::Flush)? {
            Response::Flushed(r) => Ok(r),
            _ => Err(QueueError::Invalid("unexpected response".into())),
        }
    }

    /// Latest snapshot of a page; never blocks on the writer.
    #[must_use]
    pub fn snapshot(&self, key: &PageKey) -> Option<Arc<PageSnapshot>> {
        self.inner
            .snapshots
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    /// Keys of all snapshotted pages.
    #[must_use]
    pub fn snapshot_keys(&self) -> Vec<PageKey> {
        self.inner
            .snapshots
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// A copy of the audit ring (oldest first).
    #[must_use]
    pub fn audit(&self) -> Vec<AuditEntry> {
        lock(&self.inner.audit).iter().cloned().collect()
    }

    /// Flushes pending writes and then blocks every other writer until the lock is dropped.
    /// Gives up with [`QueueError::Busy`] when the consumer does not get there within
    /// `timeout` or pages cannot be flushed (e.g. a conflict is pending).
    ///
    /// # Errors
    /// [`QueueError::Busy`], [`QueueError::Closed`].
    pub fn acquire(&self, timeout: Duration) -> Result<QueueLock, QueueError> {
        let (ready, rx) = oneshot();
        let (ltx, lrx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.tx
            .send(Msg::Acquire(AcquireMsg {
                ready,
                locks: lrx,
                cancelled: cancelled.clone(),
            }))
            .map_err(|_| QueueError::Closed)?;
        match rx.wait_timeout(timeout) {
            Ok(Some(Ok(()))) => Ok(QueueLock { tx: ltx }),
            Ok(Some(Err(e))) => Err(e),
            Ok(None) => Err(QueueError::Closed),
            Err(_) => {
                // Dropping `ltx` also releases a lock the consumer grants after we left.
                cancelled.store(true, Ordering::SeqCst);
                Err(QueueError::Busy)
            }
        }
    }
}

struct Worker {
    ws: Workspace,
    store: Box<dyn FileStore + Send>,
    config: QueueConfig,
    inner: Arc<Inner>,
    seq: u64,
    sched: Option<WriteQueue>,
    started: Instant,
}

/// Stands for "files of deleted pages are waiting to be removed" in the write queue.
const DELETES_KEY: &str = "\u{0}pending-deletes";

impl Worker {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn run(mut self, rx: &Receiver<Msg>) -> (Workspace, FlushReport) {
        // Leftovers of a crash between "create temp" and "rename".
        let _ = self.store.cleanup_stale();
        loop {
            let deadline = self.sched.as_ref().and_then(WriteQueue::next_deadline);
            let msg = match deadline {
                Some(d) => match rx.recv_timeout(d.saturating_sub(self.now())) {
                    Ok(m) => Some(m),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => break,
                },
            };
            match msg {
                Some(Msg::Command(c)) => {
                    let r = self.handle(c.source, c.request);
                    c.reply.send(r);
                }
                Some(Msg::Acquire(a)) => self.locked(a),
                Some(Msg::Shutdown) => break,
                None => {}
            }
            self.flush_due();
        }
        let report = self.ws.flush(&mut *self.store);
        self.after_flush(&report);
        (self.ws, report)
    }

    /// Writes the pages whose debounce expired (and failed ones whose backoff elapsed).
    fn flush_due(&mut self) {
        let now = self.now();
        let Some(sched) = self.sched.as_ref() else {
            return;
        };
        let due = sched.due(now);
        if due.is_empty() {
            return;
        }
        let report = self.ws.flush_pages(&mut *self.store, &due);
        self.after_flush(&report);
    }

    fn emit(&self, ev: &QueueEvent) {
        for o in &self.config.observers {
            o(ev);
        }
    }

    fn publish_all(&self) {
        let keys: Vec<PageKey> = self.ws.pages().map(|p| p.key.clone()).collect();
        self.publish(&keys);
    }

    fn publish(&self, keys: &[PageKey]) {
        let built: Vec<(PageKey, Option<Arc<PageSnapshot>>)> = keys
            .iter()
            .map(|k| (k.clone(), snapshot_of(&self.ws, k, self.seq).map(Arc::new)))
            .collect();
        let mut map = self
            .inner
            .snapshots
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (k, s) in built {
            match s {
                Some(s) => {
                    map.insert(k, s);
                }
                None => {
                    map.remove(&k);
                }
            }
        }
    }

    fn audit(
        &mut self,
        source: Source,
        label: String,
        tx: Option<TxId>,
        pages: Vec<PageKey>,
    ) -> u64 {
        self.seq += 1;
        let mut a = lock(&self.inner.audit);
        if a.len() >= self.config.audit_capacity {
            a.pop_front();
        }
        a.push_back(AuditEntry {
            seq: self.seq,
            source,
            label,
            tx,
            pages,
            at: SystemTime::now(),
        });
        self.seq
    }

    fn after_flush(&mut self, report: &FlushReport) {
        let keys: Vec<PageKey> = report
            .written
            .iter()
            .filter_map(|w| w.page.clone())
            .collect();
        self.publish(&keys);
        self.settle(report);
        if !report.written.is_empty() || !report.deleted.is_empty() || !report.backups.is_empty() {
            self.emit(&QueueEvent::Flushed(report.clone()));
        }
        if !report.conflicts.is_empty() {
            self.emit(&QueueEvent::Conflict(report.conflicts.clone()));
        }
        if !report.failed.is_empty() {
            self.emit(&QueueEvent::WriteFailed {
                failed: report.failed.clone(),
                unwritten: report.unwritten.clone(),
            });
        }
        if !report.recreated.is_empty() {
            self.emit(&QueueEvent::Recreated(report.recreated.clone()));
        }
    }

    /// Updates the write schedule after a flush: written pages are done, conflicting ones are
    /// parked, failed ones retry with backoff.
    fn settle(&mut self, report: &FlushReport) {
        let now = self.now();
        let Some(sched) = self.sched.as_mut() else {
            return;
        };
        for key in sched.all() {
            if report.conflicts.contains(&key) {
                sched.parked(&key);
            } else if report.failed.iter().any(|(k, _)| *k == key) {
                sched.failed(&key, now);
            } else if key.as_str() == DELETES_KEY {
                if self.ws.pending_deletes().is_empty() && self.ws.pending_restores().is_empty() {
                    sched.done(&key);
                }
            } else if self.ws.page(&key).is_none_or(|p| !p.needs_write()) {
                sched.done(&key);
            }
        }
        // Failures of pages that were not pending (explicit flush) also retry.
        for (key, _) in &report.failed {
            if sched.attempts(key) == 0 {
                sched.failed(key, now);
            }
        }
    }

    fn schedule(&mut self, pages: &[PageKey]) {
        let now = self.now();
        let deletes =
            !self.ws.pending_deletes().is_empty() || !self.ws.pending_restores().is_empty();
        let Some(sched) = self.sched.as_mut() else {
            return;
        };
        sched.mark(pages, now);
        if deletes {
            sched.mark(&[PageKey::from_title(DELETES_KEY)], now);
        }
    }

    fn handle(&mut self, source: Source, req: Request) -> Result<Response, QueueError> {
        match req {
            Request::Run { label, cmd } => {
                let tx = self.ws.run(label, &cmd)?;
                self.committed(source, tx)
            }
            Request::Commit { label, ops } => {
                let tx = self.ws.commit(label, ops)?;
                self.committed(source, tx)
            }
            Request::LoadPage {
                key,
                title,
                path,
                bytes,
            } => {
                if self.ws.page(&key).is_some_and(|p| p.needs_write()) {
                    return Err(QueueError::PageDirty(key));
                }
                let replaced = self.ws.page(&key).is_some();
                self.ws.load_page(key.clone(), &title, path, &bytes);
                self.seq += 1;
                self.publish(std::slice::from_ref(&key));
                if replaced {
                    self.emit(&QueueEvent::PageReloaded(key));
                }
                Ok(Response::Loaded)
            }
            Request::Flush => {
                let r = self.ws.flush(&mut *self.store);
                self.after_flush(&r);
                Ok(Response::Flushed(r))
            }
            Request::Resolve { key, keep } => {
                let now = SystemTime::now();
                match keep {
                    Keep::Mine => {
                        let r = self.ws.resolve_keep_mine(&key, &mut *self.store, now);
                        self.after_flush(&r);
                        Ok(Response::Resolved(r, None))
                    }
                    Keep::Disk => {
                        let mut r = FlushReport::default();
                        let t = self.ws.take_disk(&key, &mut *self.store, now, &mut r);
                        self.seq += 1;
                        self.publish(std::slice::from_ref(&key));
                        if let Some(s) = self.sched.as_mut() {
                            s.done(&key);
                        }
                        match t {
                            TakeDisk::Reloaded => self.emit(&QueueEvent::PageReloaded(key)),
                            TakeDisk::Removed => {
                                self.emit(&QueueEvent::PagesRemoved(vec![key]));
                            }
                            TakeDisk::Unknown => {}
                        }
                        self.after_flush(&r);
                        Ok(Response::Resolved(r, Some(t)))
                    }
                }
            }
            Request::CheckMissing { paths } => {
                let gone = self.ws.drop_missing(&*self.store, &paths);
                if !gone.is_empty() {
                    self.seq += 1;
                    self.publish(&gone);
                    self.emit(&QueueEvent::PagesRemoved(gone.clone()));
                }
                Ok(Response::Removed(gone))
            }
        }
    }

    fn committed(&mut self, source: Source, tx: Transaction) -> Result<Response, QueueError> {
        if tx.ops.is_empty() {
            return Ok(Response::Committed(tx));
        }
        let seq = self.audit(source, tx.label.to_owned(), Some(tx.id), tx.pages.clone());
        self.publish(&tx.pages);
        self.emit(&QueueEvent::Committed {
            seq,
            source,
            tx: tx.id,
            pages: tx.pages.clone(),
        });
        if self.config.auto_flush {
            let r = self.ws.flush(&mut *self.store);
            self.after_flush(&r);
        } else {
            self.schedule(&tx.pages);
        }
        Ok(Response::Committed(tx))
    }

    fn locked(&mut self, a: AcquireMsg) {
        if a.cancelled.load(Ordering::SeqCst) {
            return;
        }
        let report = self.ws.flush(&mut *self.store);
        self.after_flush(&report);
        if !report.is_complete() {
            a.ready.send(Err(QueueError::Busy));
            return;
        }
        a.ready.send(Ok(()));
        while let Ok(m) = a.locks.recv() {
            match m {
                LockMsg::Apply(edits, reply) => {
                    let r = self.apply_edits(edits);
                    reply.send(r);
                }
                LockMsg::Release => break,
            }
        }
    }

    fn apply_edits(&mut self, edits: Vec<FileEdit>) -> Result<(), QueueError> {
        let mut parsed: Vec<(GraphPath, &FileEdit)> = Vec::with_capacity(edits.len());
        for e in &edits {
            let raw = match e {
                FileEdit::Write { path, .. } | FileEdit::Delete { path, .. } => path,
            };
            let p = GraphPath::new(raw).map_err(|err| QueueError::Invalid(err.to_string()))?;
            parsed.push((p, e));
        }
        // Check every expectation before touching anything.
        for (path, e) in &parsed {
            let expected = match e {
                FileEdit::Write { expected, .. } | FileEdit::Delete { expected, .. } => expected,
            };
            let cur = self
                .store
                .read(path)
                .map_err(|err| QueueError::Store(err.to_string()))?;
            if cur != *expected {
                return Err(QueueError::Stale(path.to_string()));
            }
        }
        let mut written = Vec::new();
        let mut deleted = Vec::new();
        let mut touched: Vec<(PageKey, bool)> = Vec::new();
        for (path, e) in &parsed {
            match e {
                FileEdit::Write { content, .. } => {
                    self.store
                        .write(path, content)
                        .map_err(|err| QueueError::Store(err.to_string()))?;
                    written.push(WrittenFile {
                        page: None,
                        path: path.clone(),
                        hash: blake3::hash(content),
                    });
                    if let Some(k) = self.ws.page_for_path(path).cloned() {
                        let title = self
                            .ws
                            .page(&k)
                            .map(|p| p.title.clone())
                            .unwrap_or_default();
                        self.ws
                            .load_page(k.clone(), &title, Some(path.clone()), content);
                        touched.push((k, true));
                    }
                }
                FileEdit::Delete { expected, .. } => {
                    if expected.is_some() {
                        self.store
                            .remove(path)
                            .map_err(|err| QueueError::Store(err.to_string()))?;
                        deleted.push(path.clone());
                    }
                    if let Some(k) = self.ws.page_for_path(path).cloned() {
                        self.ws.remove_page(&k);
                        touched.push((k, false));
                    }
                }
            }
        }
        let keys: Vec<PageKey> = touched.iter().map(|(k, _)| k.clone()).collect();
        let pages = keys.clone();
        self.audit(
            Source::Sync,
            format!("apply {} file edit(s)", parsed.len()),
            None,
            pages,
        );
        self.publish(&keys);
        for (k, reloaded) in touched {
            if reloaded {
                self.emit(&QueueEvent::PageReloaded(k));
            }
        }
        self.emit(&QueueEvent::FilesApplied { written, deleted });
        Ok(())
    }
}
