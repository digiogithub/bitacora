//! The sync engine: commit, fetch, fast-forward, block-aware merge, push, offline handling
//! (BIT-SP-0006.R1/R2/R4/R5/R7, design `git-sync-merge` 2).
//!
//! [`SyncEngine`] is a synchronous state machine: every operation is a method, time comes from a
//! [`Timing`] object, and every work-tree mutation goes through the [`GraphWriter`]. It runs on
//! its own thread via [`spawn`] (a command channel plus `recv_timeout` against the next
//! deadline); no async runtime is needed.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crate::autocommit::{
    CommitOutcome, CommitRequest, CommitSettings, Debouncer, SkipReason, commit_changes,
    is_ignored_path,
};
use crate::backend::{CommitKind, CommitMessage, GitBackend, GitError, Oid, TreeChange};
use crate::commit_msg::{MessageSpec, build_message};
use crate::merge::{
    BlockLocation, ConflictRecord, ConflictType, MarkerRepair, MemoEntry, MergeInput, Resolution,
    plan_merge, repair_external_markers,
};
use crate::resolve::{ResolveError, ResolveOutcome, plan_resolution};
use crate::state::{
    Backoff, MemoryMergeStore, MergeStateStore, PendingMerge, SyncError, SyncState, SyncStatus,
};
use crate::writer::{FileChange, GraphLock, GraphWriter, WriterError};

/// Ref that keeps a pending merge (and therefore their commits) reachable.
pub const PENDING_MERGE_REF: &str = "refs/bitacora/pending-merge";

/// Clock, sleeping and jitter, injected so tests do not wait.
pub trait Timing: Send + Sync {
    /// Monotonic now.
    fn now(&self) -> Instant;
    /// Seconds since the Unix epoch (commit age checks).
    fn unix_now(&self) -> i64;
    /// Blocks for `d`.
    fn sleep(&self, d: Duration);
    /// A delay in `lo..=hi`.
    fn jitter(&self, lo: Duration, hi: Duration) -> Duration;
}

/// Wall-clock [`Timing`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemTiming;

impl Timing for SystemTiming {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn unix_now(&self) -> i64 {
        crate::autocommit::unix_now()
    }
    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
    fn jitter(&self, lo: Duration, hi: Duration) -> Duration {
        if hi <= lo {
            return lo;
        }
        let span = u64::try_from((hi - lo).as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| u64::from(d.subsec_nanos()));
        lo + Duration::from_millis(nanos % span)
    }
}

/// Engine settings (`sync.*`, design 7).
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Graph folder (work tree root).
    pub graph: PathBuf,
    /// Remote name.
    pub remote: String,
    /// Branch name.
    pub branch: String,
    /// Auto-commit settings.
    pub commit: CommitSettings,
    /// Periodic fetch while the app is focused.
    pub fetch_foreground: Duration,
    /// Periodic fetch while in the background.
    pub fetch_background: Duration,
    /// Push attempts before `Error(PushRejectedLoop)`.
    pub max_push_attempts: u32,
    /// Jitter range slept before re-fetching after a rejected push.
    pub push_retry_jitter: (Duration, Duration),
    /// Offline back-off schedule.
    pub offline_backoff: Vec<Duration>,
    /// Delay before retrying when the writer was busy.
    pub busy_retry: Duration,
}

impl EngineConfig {
    /// Defaults of the design for `graph` syncing `origin/<branch>` as `device`.
    pub fn new(graph: impl Into<PathBuf>, device: &str, branch: &str) -> Self {
        Self {
            graph: graph.into(),
            remote: "origin".to_string(),
            branch: branch.to_string(),
            commit: CommitSettings::new(device, "origin", branch),
            fetch_foreground: Duration::from_secs(120),
            fetch_background: Duration::from_secs(600),
            max_push_attempts: 5,
            push_retry_jitter: (Duration::from_secs(1), Duration::from_secs(8)),
            offline_backoff: [30, 60, 120, 300, 600]
                .into_iter()
                .map(Duration::from_secs)
                .collect(),
            busy_retry: Duration::from_secs(2),
        }
    }
}

/// Why a cycle stopped early.
#[derive(Debug)]
enum Interrupt {
    /// The writer is busy or a file changed under us; try again soon.
    Deferred,
    /// The cycle must restart from the fetch classification (new local commit).
    Restart,
    /// Hard stop with an error (becomes Offline / Error).
    Fail(GitError),
    Writer(WriterError),
    Rejected,
}

impl From<GitError> for Interrupt {
    fn from(e: GitError) -> Self {
        Self::Fail(e)
    }
}

impl From<WriterError> for Interrupt {
    fn from(e: WriterError) -> Self {
        match e {
            WriterError::Busy | WriterError::Stale(_) => Self::Deferred,
            other => Self::Writer(other),
        }
    }
}

type RefLookup = Arc<dyn Fn(&str) -> bool + Send + Sync>;
type Observer = Arc<dyn Fn(&SyncStatus) + Send + Sync>;
type BlockLocator = Arc<dyn Fn(&str) -> Option<BlockLocation> + Send + Sync>;

/// The sync engine for one graph.
pub struct SyncEngine {
    backend: Box<dyn GitBackend>,
    writer: Arc<dyn GraphWriter>,
    store: Box<dyn MergeStateStore>,
    config: EngineConfig,
    timing: Arc<dyn Timing>,
    state: SyncState,
    debouncer: Debouncer,
    backoff: Backoff,
    next_fetch: Instant,
    retry_at: Option<Instant>,
    focused: bool,
    last_sync: Option<SystemTime>,
    last_error: Option<String>,
    ahead: usize,
    is_referenced: RefLookup,
    locator: Option<BlockLocator>,
    template: Option<String>,
    marker_seq: usize,
    observer: Option<Observer>,
    shared: Arc<Mutex<SyncStatus>>,
    history: Vec<SyncState>,
    upstream_set: bool,
    agent_pending: Option<String>,
}

impl SyncEngine {
    /// Creates an engine. A pending merge found in `store` restores `Conflicted` (recovery).
    pub fn new(
        backend: Box<dyn GitBackend>,
        writer: Arc<dyn GraphWriter>,
        store: Box<dyn MergeStateStore>,
        config: EngineConfig,
        timing: Arc<dyn Timing>,
    ) -> Self {
        let now = timing.now();
        let state = if store.load().is_some() {
            SyncState::Conflicted
        } else {
            SyncState::Idle
        };
        let status = SyncStatus {
            state: state.clone(),
            ahead: 0,
            last_sync: None,
            conflicts: store.load().map_or(0, |p| p.unresolved()),
            last_error: None,
            backend: backend.kind(),
        };
        // A configured idle below the documented floor is only used by tests.
        let floor = config.commit.idle.min(crate::autocommit::MIN_IDLE);
        let debouncer = Debouncer::with_floor(config.commit.idle, config.commit.max, floor);
        let backoff = Backoff::new(config.offline_backoff.clone());
        let next_fetch = now + config.fetch_foreground;
        Self {
            backend,
            writer,
            store,
            config,
            timing,
            state,
            debouncer,
            backoff,
            next_fetch,
            retry_at: None,
            focused: true,
            last_sync: None,
            last_error: None,
            ahead: 0,
            is_referenced: Arc::new(|_| false),
            locator: None,
            template: None,
            marker_seq: 0,
            observer: None,
            shared: Arc::new(Mutex::new(status)),
            history: Vec::new(),
            upstream_set: false,
            agent_pending: None,
        }
    }

    /// Engine with an in-memory merge store and the system clock.
    pub fn with_defaults(
        backend: Box<dyn GitBackend>,
        writer: Arc<dyn GraphWriter>,
        config: EngineConfig,
    ) -> Self {
        Self::new(
            backend,
            writer,
            Box::new(MemoryMergeStore::new()),
            config,
            Arc::new(SystemTiming),
        )
    }

    /// Sets the "is this block uuid referenced anywhere" lookup used by the `id::` union rule.
    pub fn set_reference_lookup(&mut self, f: RefLookup) {
        self.is_referenced = f;
    }

    /// Sets the lookup that says which block carries a uuid, so a merge that newly references a
    /// block writes its `id::` in the same merge commit (BIT-SP-0006.R14).
    pub fn set_block_locator(&mut self, f: BlockLocator) {
        self.locator = Some(f);
    }

    /// Sets the default journal template text, so a template-only journal counts as unchanged in
    /// add/add merges (BIT-SP-0006.R18).
    pub fn set_journal_template(&mut self, template: Option<String>) {
        self.template = template;
    }

    /// Registers a callback invoked on every state change.
    pub fn set_observer(&mut self, f: Observer) {
        self.observer = Some(f);
    }

    /// Shared status cell, updated on every state change.
    pub fn shared_status(&self) -> Arc<Mutex<SyncStatus>> {
        Arc::clone(&self.shared)
    }

    /// The current state.
    pub fn state(&self) -> &SyncState {
        &self.state
    }

    /// Every state entered since creation (bounded), for tests and diagnostics.
    pub fn history(&self) -> &[SyncState] {
        &self.history
    }

    /// Current status snapshot.
    pub fn status(&self) -> SyncStatus {
        SyncStatus {
            state: self.state.clone(),
            ahead: self.ahead,
            last_sync: self.last_sync,
            conflicts: self.store.load().map_or(0, |p| p.unresolved()),
            last_error: self.last_error.clone(),
            backend: self.backend.kind(),
        }
    }

    /// The pending merge, if any.
    pub fn pending_merge(&self) -> Option<PendingMerge> {
        self.store.load()
    }

    fn set_state(&mut self, to: SyncState) {
        if self.state == to {
            return;
        }
        debug_assert!(
            self.state.can_transition(&to),
            "illegal sync transition {:?} -> {:?}",
            self.state,
            to
        );
        self.state = to.clone();
        if self.history.len() >= 512 {
            self.history.drain(..256);
        }
        self.history.push(to);
        self.publish();
    }

    fn publish(&self) {
        let status = self.status();
        if let Ok(mut g) = self.shared.lock() {
            *g = status.clone();
        }
        if let Some(o) = &self.observer {
            o(&status);
        }
    }

    // ---- triggers -------------------------------------------------------------------------

    /// A write was flushed to disk (editor, MCP, watcher): restarts the idle debounce.
    pub fn note_write(&mut self) {
        let now = self.timing.now();
        self.debouncer.note_write(now);
        if self.state == SyncState::Idle {
            self.set_state(SyncState::Dirty);
        }
    }

    /// An MCP agent wrote to the graph: the next automatic commit is recorded as
    /// `Bitacora-Kind: agent` with a `Bitacora-Agent` trailer naming the client, and is never
    /// squashed into a user's auto commit. Also counts as a write for the idle debounce.
    pub fn note_agent_write(&mut self, agent: &str) {
        self.agent_pending = Some(agent.to_owned());
        self.note_write();
    }

    /// The request for the next automatic commit (consumes a pending agent attribution).
    fn auto_request(&mut self) -> CommitRequest {
        match self.agent_pending.take() {
            Some(agent) => CommitRequest {
                kind: CommitKind::Agent,
                agent: Some(agent),
                subject: None,
            },
            None => CommitRequest::auto(),
        }
    }

    /// Foreground/background switch (changes the periodic fetch interval).
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        let now = self.timing.now();
        self.next_fetch = now + self.fetch_interval();
    }

    /// The OS reported the network is up (or the machine woke): resets the back-off.
    pub fn network_changed(&mut self) {
        self.backoff.reset();
        self.retry_at = None;
    }

    fn fetch_interval(&self) -> Duration {
        if self.focused {
            self.config.fetch_foreground
        } else {
            self.config.fetch_background
        }
    }

    /// The next instant the engine wants to run, if any.
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.state == SyncState::Disabled {
            return None;
        }
        let mut d = Some(self.next_fetch);
        for cand in [self.debouncer.deadline(), self.retry_at]
            .into_iter()
            .flatten()
        {
            d = Some(d.map_or(cand, |x| x.min(cand)));
        }
        d
    }

    /// Runs whatever is due at the current time.
    pub fn on_timer(&mut self) {
        let now = self.timing.now();
        let due = self.debouncer.is_due(now)
            || self.retry_at.is_some_and(|t| now >= t)
            || now >= self.next_fetch;
        if due {
            self.sync_now();
        }
    }

    /// User disabled sync.
    pub fn disable(&mut self) {
        self.set_state(SyncState::Disabled);
    }

    /// User (re-)enabled sync.
    pub fn enable(&mut self) {
        if self.state == SyncState::Disabled {
            self.set_state(SyncState::Idle);
            if self.store.load().is_some() {
                self.set_state(SyncState::Conflicted);
            }
        }
    }

    // ---- commit ---------------------------------------------------------------------------

    /// Commits local changes now (manual commit, graph open/close, agent writes).
    pub fn commit_now(&mut self, request: &CommitRequest) -> Result<CommitOutcome, GitError> {
        let before = self.state.clone();
        self.set_state(SyncState::Committing);
        self.debouncer.clear();
        let result = self.commit_under_lock(request);
        self.restore_resting_state(&before);
        result
    }

    fn commit_under_lock(&mut self, request: &CommitRequest) -> Result<CommitOutcome, GitError> {
        let lock = self
            .writer
            .acquire()
            .map_err(|e| GitError::other(format!("graph writer: {e}")))?;
        let out = commit_changes(
            self.backend.as_ref(),
            &self.config.commit,
            request,
            self.timing.unix_now(),
        );
        drop(lock);
        if out.is_ok() {
            self.refresh_ahead();
        }
        out
    }

    fn restore_resting_state(&mut self, before: &SyncState) {
        let rest = if self.store.load().is_some() {
            SyncState::Conflicted
        } else if matches!(before, SyncState::Offline | SyncState::Error(_)) {
            before.clone()
        } else if self.debouncer.is_dirty() {
            SyncState::Dirty
        } else {
            SyncState::Idle
        };
        self.set_state(rest);
    }

    // ---- sync cycle -----------------------------------------------------------------------

    /// Commit (if needed), fetch, integrate and push; returns the resulting state.
    pub fn sync_now(&mut self) -> SyncState {
        self.run_cycle(self.config.max_push_attempts)
    }

    /// Graph/app close: commit, then one best-effort push attempt (design 2.1).
    pub fn close(&mut self) -> SyncState {
        self.run_cycle(1)
    }

    fn run_cycle(&mut self, attempts: u32) -> SyncState {
        if self.state == SyncState::Disabled {
            return self.state.clone();
        }
        self.debouncer.clear();
        let result = self.cycle(attempts);
        let now = self.timing.now();
        self.next_fetch = now + self.fetch_interval();
        match result {
            Ok(()) => {
                self.retry_at = None;
            }
            Err(Interrupt::Deferred) => {
                // Writer busy or file changed: stay dirty and retry soon.
                self.retry_at = Some(now + self.config.busy_retry);
                self.debouncer.note_write(now);
                let rest = if self.store.load().is_some() {
                    SyncState::Conflicted
                } else {
                    SyncState::Dirty
                };
                self.set_state(rest);
            }
            Err(Interrupt::Restart) => {
                self.retry_at = Some(now);
                self.set_state(SyncState::Dirty);
            }
            Err(Interrupt::Rejected) => {
                self.last_error = Some(SyncError::PushRejectedLoop.to_string());
                self.set_state(SyncState::Error(SyncError::PushRejectedLoop));
            }
            Err(Interrupt::Writer(e)) => {
                let err = SyncError::Writer(e.to_string());
                self.last_error = Some(err.to_string());
                self.set_state(SyncState::Error(err));
            }
            Err(Interrupt::Fail(e)) => self.fail(e),
        }
        self.refresh_ahead();
        self.publish();
        self.state.clone()
    }

    fn fail(&mut self, e: GitError) {
        self.last_error = Some(e.to_string());
        match e {
            GitError::Network(_) => {
                let delay = self.backoff.next_delay();
                self.retry_at = Some(self.timing.now() + delay);
                self.set_state(SyncState::Offline);
            }
            GitError::Auth { hint } => {
                self.set_state(SyncState::Error(SyncError::Auth(
                    hint.unwrap_or_else(|| "check credentials".to_string()),
                )));
            }
            GitError::ExternalOperationInProgress => {
                self.set_state(SyncState::Error(SyncError::ExternalOperationInProgress));
            }
            other => self.set_state(SyncState::Error(SyncError::Git(other.to_string()))),
        }
    }

    fn branch_ref(&self) -> String {
        format!("refs/heads/{}", self.config.branch)
    }

    fn tracking_ref(&self) -> String {
        format!("refs/remotes/{}/{}", self.config.remote, self.config.branch)
    }

    fn check_external_operation(&self) -> Result<(), Interrupt> {
        let Some(git_dir) = crate::repo_setup::git_dir_of(&self.config.graph) else {
            return Ok(());
        };
        for marker in [
            "MERGE_HEAD",
            "rebase-merge",
            "rebase-apply",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
        ] {
            if git_dir.join(marker).exists() {
                return Err(Interrupt::Fail(GitError::ExternalOperationInProgress));
            }
        }
        Ok(())
    }

    fn cycle(&mut self, attempts: u32) -> Result<(), Interrupt> {
        self.take_over_pull_merge()?;
        self.check_external_operation()?;
        self.repair_markers()?;
        // 1. Commit local work (Committing -> Syncing, or back to rest when nothing changed).
        let before = self.state.clone();
        self.set_state(SyncState::Committing);
        let request = self.auto_request();
        let committed = {
            let lock = self.writer.acquire()?;
            let out = commit_changes(
                self.backend.as_ref(),
                &self.config.commit,
                &request,
                self.timing.unix_now(),
            );
            drop(lock);
            out?
        };
        match committed {
            CommitOutcome::Committed { .. } => self.set_state(SyncState::Syncing),
            CommitOutcome::Skipped(SkipReason::Clean | SkipReason::OnlyIgnored) => {
                self.restore_resting_state(&before);
                let next = if self.store.load().is_some() {
                    SyncState::Conflicted
                } else {
                    SyncState::Syncing
                };
                // Idle/Offline/Error/Dirty -> Syncing is allowed; Conflicted fetches directly.
                self.set_state(next);
            }
        }
        // 2. Fetch, integrate, push (bounded retry on rejected pushes).
        self.set_state(SyncState::Fetching);
        let mut attempt = 1;
        let mut restarts = 0;
        loop {
            match self.step() {
                Ok(StepResult::Done) => return Ok(()),
                Ok(StepResult::Rejected) => {
                    if attempt >= attempts {
                        return Err(Interrupt::Rejected);
                    }
                    attempt += 1;
                    let (lo, hi) = self.config.push_retry_jitter;
                    let d = self.timing.jitter(lo, hi);
                    self.timing.sleep(d);
                    self.set_state(SyncState::Fetching);
                }
                Err(Interrupt::Restart) if restarts < 5 => {
                    restarts += 1;
                    self.set_state(SyncState::Fetching);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// One fetch-classify-integrate-push pass. Starts in `Fetching`.
    fn step(&mut self) -> Result<StepResult, Interrupt> {
        let remote_head = match self.backend.fetch(&self.config.remote, &self.config.branch) {
            Ok(f) => f.remote_head,
            Err(GitError::RemoteRefNotFound(_)) => None,
            Err(e) => return Err(e.into()),
        };
        let status = self.backend.status()?;
        if !status.unmerged.is_empty() {
            return Err(GitError::ExternalOperationInProgress.into());
        }
        let Some(local) = status.head else {
            return Err(GitError::other("the graph repository has no commits").into());
        };
        if status.branch.as_deref() != Some(self.config.branch.as_str()) {
            return Err(GitError::other(format!(
                "HEAD is on `{}` but sync is configured for `{}`",
                status.branch.as_deref().unwrap_or("(detached)"),
                self.config.branch
            ))
            .into());
        }

        let pending = self.store.load();
        let Some(remote) = remote_head else {
            return self.push_phase();
        };
        if remote == local {
            self.clear_pending_if_done();
            self.set_state(SyncState::Idle);
            self.mark_synced();
            return Ok(StepResult::Done);
        }
        if let Some(p) = &pending
            && p.theirs == remote
        {
            // Remote did not move since the merge was computed: keep waiting for the user.
            self.set_state(SyncState::Conflicted);
            return Ok(StepResult::Done);
        }
        let base = self.backend.merge_base(&local, &remote)?;
        if base.as_ref() == Some(&remote) && pending.is_none() {
            return self.push_phase();
        }
        if base.as_ref() == Some(&remote) {
            // Remote is an ancestor of HEAD although a merge was pending: the user's history
            // already contains it. Nothing left to resolve.
            self.clear_pending_if_done();
            return self.push_phase();
        }
        self.set_state(SyncState::Integrating);
        if base.as_ref() == Some(&local) && !self.remote_adds_markers(&local, &remote)? {
            self.set_state(SyncState::FastForward);
            self.fast_forward(&local, &remote)?;
            self.clear_pending_if_done();
            self.set_state(SyncState::Idle);
            self.mark_synced();
            return Ok(StepResult::Done);
        }
        self.set_state(SyncState::Merging);
        match self.merge(&local, &remote, base.as_ref())? {
            MergeResult::Merged => self.push_phase(),
            MergeResult::Conflicted => {
                self.set_state(SyncState::Conflicted);
                Ok(StepResult::Done)
            }
        }
    }

    /// Whether the remote commit brings text files with conflict marker lines. Such a fast-forward
    /// goes through the merge instead, which splits the regions (R8).
    fn remote_adds_markers(&self, local: &Oid, remote: &Oid) -> Result<bool, GitError> {
        use crate::merge::policy::{Policy, policy_for};
        for change in self.backend.diff_trees(local, remote)? {
            let path = match &change {
                TreeChange::Added { path } | TreeChange::Modified { path } => path,
                TreeChange::Renamed { to, .. } => to,
                TreeChange::Deleted { .. } => continue,
            };
            let Some(bytes) = self.backend.read_blob(remote, path)? else {
                continue;
            };
            let policy = policy_for(path, [None, None, Some(&bytes)]);
            if matches!(
                policy,
                Policy::Markdown | Policy::TextLine | Policy::Config | Policy::WholeFile
            ) && std::str::from_utf8(&bytes).is_ok_and(crate::merge::markers::has_marker_line)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn push_phase(&mut self) -> Result<StepResult, Interrupt> {
        if self.store.load().is_some() {
            // Nothing is pushed while conflicts are unresolved.
            self.set_state(SyncState::Conflicted);
            return Ok(StepResult::Done);
        }
        self.set_state(SyncState::Pushing);
        match self.backend.push(&self.config.remote, &self.config.branch) {
            Ok(_) => {
                self.set_upstream_once();
                self.set_state(SyncState::Idle);
                self.mark_synced();
                Ok(StepResult::Done)
            }
            Err(GitError::NonFastForward) => Ok(StepResult::Rejected),
            Err(e) => Err(e.into()),
        }
    }

    fn mark_synced(&mut self) {
        self.last_sync = Some(SystemTime::now());
        self.last_error = None;
        self.backoff.reset();
        self.retry_at = None;
    }

    fn set_upstream_once(&mut self) {
        if self.upstream_set {
            return;
        }
        self.upstream_set = true;
        if let Some(git_dir) = crate::repo_setup::git_dir_of(&self.config.graph) {
            let _ =
                crate::onboarding::set_upstream(&git_dir, &self.config.remote, &self.config.branch);
        }
    }

    fn acquire_clean<'w>(
        &self,
        writer: &'w dyn GraphWriter,
        local: &Oid,
    ) -> Result<Box<dyn GraphLock + 'w>, Interrupt> {
        let lock = writer.acquire()?;
        // The flush done by `acquire` may have written files after the commit step: fold them in
        // and restart so the merge works on the final local tip.
        let status = self.backend.status()?;
        if status.head.as_ref() != Some(local) {
            return Err(Interrupt::Restart);
        }
        if status.dirty.iter().any(|d| !is_ignored_path(&d.path)) {
            commit_changes(
                self.backend.as_ref(),
                &self.config.commit,
                &CommitRequest::auto(),
                self.timing.unix_now(),
            )?;
            return Err(Interrupt::Restart);
        }
        Ok(lock)
    }

    fn fast_forward(&mut self, local: &Oid, remote: &Oid) -> Result<(), Interrupt> {
        let writer = Arc::clone(&self.writer);
        let mut lock = self.acquire_clean(&*writer, local)?;
        let mut changes = Vec::new();
        for change in self.backend.diff_trees(local, remote)? {
            match change {
                TreeChange::Added { path } => {
                    if let Some(content) = self.backend.read_blob(remote, &path)? {
                        changes.push(FileChange::Write {
                            path,
                            content,
                            expected: None,
                        });
                    }
                }
                TreeChange::Modified { path } => {
                    let expected = self.backend.read_blob(local, &path)?;
                    if let Some(content) = self.backend.read_blob(remote, &path)? {
                        changes.push(FileChange::Write {
                            path,
                            content,
                            expected,
                        });
                    }
                }
                TreeChange::Deleted { path } => {
                    let expected = self.backend.read_blob(local, &path)?;
                    changes.push(FileChange::Delete { path, expected });
                }
                TreeChange::Renamed { from, to, .. } => {
                    let expected = self.backend.read_blob(local, &from)?;
                    changes.push(FileChange::Delete {
                        path: from,
                        expected,
                    });
                    if let Some(content) = self.backend.read_blob(remote, &to)? {
                        changes.push(FileChange::Write {
                            path: to,
                            content,
                            expected: None,
                        });
                    }
                }
            }
        }
        for c in &changes {
            if !crate::writer::is_safe_relative(c.path()) {
                return Err(GitError::other(format!("unsafe path in tree: `{}`", c.path())).into());
            }
        }
        // Files first, then the ref: a crash in between leaves a work tree that is ahead of HEAD
        // (harmless: the next commit records it), never one that silently reverts remote work.
        lock.apply(&changes)?;
        self.backend
            .update_ref(&self.branch_ref(), remote, Some(local))?;
        self.backend.reset_index(remote)?;
        drop(lock);
        Ok(())
    }

    fn merge(
        &mut self,
        local: &Oid,
        remote: &Oid,
        base: Option<&Oid>,
    ) -> Result<MergeResult, Interrupt> {
        let writer = Arc::clone(&self.writer);
        let mut lock = self.acquire_clean(&*writer, local)?;
        let previous = self.store.load();
        let memo: Vec<MemoEntry> = previous
            .as_ref()
            .map(|p| p.memo.clone())
            .unwrap_or_default();
        // Conflicts that come from external marker files survive a recomputed merge.
        let carried: Vec<ConflictRecord> = previous
            .as_ref()
            .filter(|p| p.external)
            .map(|p| p.conflicts.clone())
            .unwrap_or_default();
        let plan = {
            let lookup = Arc::clone(&self.is_referenced);
            let locator = self.locator.clone();
            let locate = locator.as_ref().map(|l| {
                let l = Arc::clone(l);
                move |u: &str| l(u)
            });
            let mut input = MergeInput::new(self.backend.as_ref(), local, remote, base, &*lookup);
            input.memo = &memo;
            input.template = self.template.as_deref();
            input.locate_block = locate
                .as_ref()
                .map(|f| f as &dyn Fn(&str) -> Option<BlockLocation>);
            plan_merge(&input)?
        };
        let tree = self.backend.write_tree(local, &plan.edits)?;
        let open = carried.iter().filter(|c| c.is_unresolved()).count()
            + plan.conflicts.iter().filter(|c| c.is_unresolved()).count();
        if open == 0 {
            let had_conflicts = !plan.conflicts.is_empty();
            let kind = if had_conflicts {
                CommitKind::Resolve
            } else {
                CommitKind::Merge
            };
            let mut msg = build_message(&MessageSpec {
                kind,
                device: &self.config.commit.device,
                pages: &plan.touched,
                agent: None,
                subject: Some(&format!(
                    "bitacora: merge {}/{}",
                    self.config.remote, self.config.branch
                )),
            });
            for note in plan.notes.iter().take(20) {
                msg = msg.trailer("Bitacora-Note", note.replace('\n', " "));
            }
            let merge = self
                .backend
                .commit_tree(&tree, &[local.clone(), remote.clone()], &msg)?;
            lock.apply(&plan.changes)?;
            self.backend
                .update_ref(&self.branch_ref(), &merge, Some(local))?;
            self.backend.reset_index(&merge)?;
            drop(lock);
            self.store.clear();
            Ok(MergeResult::Merged)
        } else {
            // Keep their commits reachable and the merged tree recoverable.
            let msg = CommitMessage::new("bitacora: pending merge (unresolved conflicts)")
                .trailer("Bitacora-Device", self.config.commit.device.clone())
                .kind(CommitKind::Merge);
            let pending_commit =
                self.backend
                    .commit_tree(&tree, &[local.clone(), remote.clone()], &msg)?;
            let previous_ref = self.backend.resolve_ref(PENDING_MERGE_REF)?;
            self.backend
                .update_ref(PENDING_MERGE_REF, &pending_commit, previous_ref.as_ref())?;
            // The work tree gets every auto-resolved remote change; conflicting regions keep ours.
            lock.apply(&plan.changes)?;
            drop(lock);
            let mut conflicts = carried;
            conflicts.extend(plan.conflicts);
            self.store.save(&PendingMerge {
                base: base.cloned(),
                ours: local.clone(),
                theirs: remote.clone(),
                pending_commit,
                conflicts,
                memo,
                external: false,
            });
            Ok(MergeResult::Conflicted)
        }
    }

    /// Applies the user's `resolution` of conflict `id` to the work tree through the graph writer,
    /// records it (and its memo, so it survives a recomputed merge) in the persisted state and,
    /// when it was the last open conflict, finishes the merge with the `resolve` commit and
    /// pushes. This is the call the visual resolver (BIT-US-0054) makes.
    pub fn resolve_conflict(
        &mut self,
        id: &str,
        resolution: Resolution,
    ) -> Result<ResolveOutcome, ResolveError> {
        let mut pending = self.store.load().ok_or(ResolveError::NoPendingMerge)?;
        let Some(idx) = pending.conflicts.iter().position(|c| c.id == id) else {
            return Err(ResolveError::UnknownConflict(id.to_owned()));
        };
        if !pending.conflicts[idx].is_unresolved() {
            return Err(ResolveError::AlreadyResolved(id.to_owned()));
        }
        let rec = pending.conflicts[idx].clone();
        let graph = self.config.graph.clone();
        let read = move |p: &str| -> Option<Vec<u8>> {
            if crate::writer::is_safe_relative(p) {
                std::fs::read(graph.join(p)).ok()
            } else {
                None
            }
        };
        let changes = plan_resolution(&read, &rec, &resolution)?;
        if !changes.is_empty() {
            let writer = Arc::clone(&self.writer);
            let mut lock = writer
                .acquire()
                .map_err(|e| ResolveError::Busy(e.to_string()))?;
            lock.apply(&changes).map_err(|e| match e {
                WriterError::Busy | WriterError::Stale(_) => ResolveError::Busy(e.to_string()),
                other => ResolveError::Failed(other.to_string()),
            })?;
        }
        pending.conflicts[idx].resolution = Some(resolution.clone());
        let entry = MemoEntry::of(&pending.conflicts[idx], &resolution);
        pending.memo.retain(|m| {
            !(m.path == entry.path
                && m.kind == entry.kind
                && m.block_key == entry.block_key
                && m.field == entry.field
                && m.base_hash == entry.base_hash
                && m.theirs_hash == entry.theirs_hash)
        });
        pending.memo.push(entry);
        let remaining = pending.unresolved();
        self.store.save(&pending);
        self.publish();
        if remaining > 0 {
            return Ok(ResolveOutcome {
                remaining,
                state: self.state.clone(),
            });
        }
        let state = self
            .finish_pending_merge()
            .map_err(|e| ResolveError::Failed(e.to_string()))?;
        Ok(ResolveOutcome {
            remaining: 0,
            state,
        })
    }

    /// Resolves every open conflict of `path` with `resolution` ("resolve all on this page with
    /// mine/theirs"). Returns the outcome of the last resolution; stops at the first error.
    pub fn resolve_page(
        &mut self,
        path: &str,
        resolution: &Resolution,
    ) -> Result<ResolveOutcome, ResolveError> {
        let pending = self.store.load().ok_or(ResolveError::NoPendingMerge)?;
        let ids: Vec<String> = pending
            .conflicts
            .iter()
            .filter(|c| c.path == path && c.is_unresolved())
            .map(|c| c.id.clone())
            .collect();
        let mut outcome = ResolveOutcome {
            remaining: pending.unresolved(),
            state: self.state.clone(),
        };
        for id in ids {
            outcome = self.resolve_conflict(&id, resolution.clone())?;
        }
        Ok(outcome)
    }

    /// Finishes the pending merge: the user's resolutions are already in the work tree (written
    /// through the core writer). Commits them and creates the two-parent `resolve` commit against
    /// the remote tip of the pending merge (a plain `resolve` commit when the conflicts came from
    /// external marker files), then syncs, which pushes.
    pub fn finish_pending_merge(&mut self) -> Result<SyncState, GitError> {
        let Some(pending) = self.store.load() else {
            return Ok(self.state.clone());
        };
        self.set_state(SyncState::Committing);
        {
            let lock = self
                .writer
                .acquire()
                .map_err(|e| GitError::other(format!("graph writer: {e}")))?;
            let request = CommitRequest {
                kind: CommitKind::Resolve,
                agent: None,
                subject: Some("bitacora: resolve merge conflicts".to_string()),
            };
            // Resolve commits are never squashed (only `auto` is), so a plain commit step is right.
            let _ = commit_changes(
                self.backend.as_ref(),
                &self.config.commit,
                &request,
                self.timing.unix_now(),
            )?;
            if !pending.external {
                let status = self.backend.status()?;
                let head = status
                    .head
                    .ok_or_else(|| GitError::other("no commit after resolve"))?;
                let info = self.backend.commit_info(&head)?;
                let msg = build_message(&MessageSpec {
                    kind: CommitKind::Resolve,
                    device: &self.config.commit.device,
                    pages: &[],
                    agent: None,
                    subject: Some("bitacora: resolve merge conflicts"),
                });
                let resolved = self.backend.commit_tree(
                    &info.tree,
                    &[head.clone(), pending.theirs.clone()],
                    &msg,
                )?;
                self.backend
                    .update_ref(&self.branch_ref(), &resolved, Some(&head))?;
                self.backend.reset_index(&resolved)?;
            }
            drop(lock);
        }
        self.store.clear();
        self.set_state(SyncState::Idle);
        Ok(self.run_cycle(self.config.max_push_attempts))
    }

    /// Clears the pending merge unless it still holds unresolved conflicts from external marker
    /// files (those are not tied to a remote commit and must survive a sync).
    fn clear_pending_if_done(&mut self) {
        if self
            .store
            .load()
            .is_some_and(|p| p.external && p.unresolved() > 0)
        {
            return;
        }
        self.store.clear();
    }

    /// A `git pull` run by the user that stopped with conflicts (`MERGE_HEAD` plus unmerged index
    /// entries) is taken over when it merges the branch we sync (R8): the half-done merge state is
    /// dropped, the work tree is kept as it is, and the normal cycle (marker repair, commit,
    /// fetch, block merge) finishes the job. Other merges, rebases, cherry-picks and reverts are
    /// still reported as `ExternalOperationInProgress`.
    fn take_over_pull_merge(&mut self) -> Result<(), Interrupt> {
        let Some(git_dir) = crate::repo_setup::git_dir_of(&self.config.graph) else {
            return Ok(());
        };
        let merge_head = git_dir.join("MERGE_HEAD");
        if !merge_head.exists()
            || [
                "rebase-merge",
                "rebase-apply",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
            ]
            .iter()
            .any(|m| git_dir.join(m).exists())
        {
            return Ok(());
        }
        let Ok(text) = std::fs::read_to_string(&merge_head) else {
            return Ok(());
        };
        let Ok(theirs) = Oid::from_hex(text.lines().next().unwrap_or("").trim()) else {
            return Ok(());
        };
        if self.backend.resolve_ref(&self.tracking_ref())?.as_ref() != Some(&theirs) {
            return Ok(());
        }
        let status = self.backend.status()?;
        let Some(head) = status.head else {
            return Ok(());
        };
        if status.unmerged.is_empty() {
            return Ok(());
        }
        for f in [
            "MERGE_HEAD",
            "MERGE_MSG",
            "MERGE_MODE",
            "AUTO_MERGE",
            "SQUASH_MSG",
        ] {
            let _ = std::fs::remove_file(git_dir.join(f));
        }
        self.backend.reset_index(&head)?;
        Ok(())
    }

    /// Marker guard (R8): files with marker regions written by another tool are split into
    /// base/ours/theirs, merged and rewritten clean through the writer; what cannot be split, or
    /// still conflicts, is registered as conflicts and nothing is pushed until it is resolved.
    /// Unresolved `external_markers` records whose file was fixed by hand count as edited.
    fn repair_markers(&mut self) -> Result<(), Interrupt> {
        let status = self.backend.status()?;
        let Some(head) = status.head.clone() else {
            return Ok(());
        };
        let mut paths: Vec<String> = status
            .dirty
            .iter()
            .filter(|d| !matches!(d.kind, crate::backend::DirtyKind::Deleted))
            .map(|d| d.path.clone())
            .chain(status.unmerged.iter().map(|u| u.path.clone()))
            .collect();
        let pending = self.store.load();
        if let Some(p) = &pending {
            paths.extend(
                p.conflicts
                    .iter()
                    .filter(|c| c.kind == ConflictType::ExternalMarkers && c.is_unresolved())
                    .map(|c| c.path.clone()),
            );
        }
        paths.sort();
        paths.dedup();
        let lookup = Arc::clone(&self.is_referenced);
        let env = bitacora_merge::MergeEnv {
            is_referenced: &*lookup,
            template: self.template.as_deref(),
            ..bitacora_merge::MergeEnv::new()
        };
        let mut changes: Vec<FileChange> = Vec::new();
        let mut found: Vec<ConflictRecord> = Vec::new();
        let mut fixed: Vec<(String, String)> = Vec::new();
        for path in paths {
            if is_ignored_path(&path) || !crate::writer::is_safe_relative(&path) {
                continue;
            }
            let Ok(bytes) = std::fs::read(self.config.graph.join(&path)) else {
                continue;
            };
            let Ok(text) = String::from_utf8(bytes.clone()) else {
                continue;
            };
            if !crate::merge::markers::has_marker_line(&text) {
                fixed.push((path, text));
                continue;
            }
            match repair_external_markers(&path, &text, &env) {
                MarkerRepair::Clean => {}
                MarkerRepair::Repaired {
                    text: clean,
                    conflicts,
                } => {
                    changes.push(FileChange::Write {
                        path: path.clone(),
                        content: clean.into_bytes(),
                        expected: Some(bytes),
                    });
                    found.extend(conflicts.into_iter().map(|mut c| {
                        c.path.clone_from(&path);
                        c
                    }));
                }
                MarkerRepair::Malformed => found.push(ConflictRecord {
                    id: String::new(),
                    path: path.clone(),
                    kind: ConflictType::ExternalMarkers,
                    field: Some("file".to_owned()),
                    block_key: None,
                    breadcrumb: Vec::new(),
                    base: None,
                    ours: Some(text),
                    theirs: None,
                    deleted_by: None,
                    suggestion: None,
                    ours_commit: Some(head.as_hex().to_owned()),
                    theirs_commit: None,
                    theirs_author: None,
                    theirs_time: None,
                    resolution: None,
                }),
            }
        }
        if !changes.is_empty() {
            let writer = Arc::clone(&self.writer);
            let mut lock = writer.acquire()?;
            lock.apply(&changes)?;
        }
        let have_pending = pending.is_some();
        if found.is_empty() && !have_pending {
            return Ok(());
        }
        let mut state = pending.unwrap_or_else(|| PendingMerge {
            base: None,
            ours: head.clone(),
            theirs: head.clone(),
            pending_commit: head.clone(),
            conflicts: Vec::new(),
            memo: Vec::new(),
            external: true,
        });
        let before = state.clone();
        for (path, text) in fixed {
            for c in state.conflicts.iter_mut().filter(|c| {
                c.kind == ConflictType::ExternalMarkers && c.is_unresolved() && c.path == path
            }) {
                c.resolution = Some(Resolution::Edit(text.clone()));
            }
        }
        let mut next = state
            .conflicts
            .iter()
            .filter_map(|c| c.id.strip_prefix("e-")?.parse::<usize>().ok())
            .max()
            .unwrap_or(0)
            .max(self.marker_seq);
        for mut rec in found {
            let duplicate = state.conflicts.iter().any(|c| {
                c.is_unresolved()
                    && c.path == rec.path
                    && c.kind == rec.kind
                    && c.field == rec.field
                    && c.block_key == rec.block_key
            });
            if duplicate {
                continue;
            }
            next += 1;
            rec.id = format!("e-{next:04}");
            state.conflicts.push(rec);
        }
        self.marker_seq = next;
        if state.external && state.unresolved() == 0 {
            // Everything came from marker files and all of it is fixed: nothing to wait for.
            self.store.clear();
            self.publish();
            return Ok(());
        }
        if state != before || !have_pending {
            if state.conflicts.is_empty() {
                return Ok(());
            }
            self.store.save(&state);
            self.publish();
        }
        Ok(())
    }

    fn refresh_ahead(&mut self) {
        self.ahead = self.count_ahead().unwrap_or(self.ahead);
    }

    /// Local commits that are not on the remote-tracking branch.
    fn count_ahead(&self) -> Result<usize, GitError> {
        let Some(head) = self.backend.resolve_ref("HEAD")? else {
            return Ok(0);
        };
        let remote = self.backend.resolve_ref(&self.tracking_ref())?;
        let mut cur = head;
        let mut n = 0;
        while n < 500 {
            if let Some(r) = &remote
                && (r == &cur || self.backend.merge_base(&cur, r)?.as_ref() == Some(&cur))
            {
                break;
            }
            n += 1;
            let info = self.backend.commit_info(&cur)?;
            match info.parents.first() {
                Some(p) => cur = p.clone(),
                None => break,
            }
        }
        Ok(n)
    }
}

enum StepResult {
    Done,
    Rejected,
}

enum MergeResult {
    Merged,
    Conflicted,
}

// ---- actor ---------------------------------------------------------------------------------

/// Messages accepted by the engine thread.
#[derive(Debug)]
pub enum Command {
    /// A write was flushed to disk.
    FileFlushed,
    /// An MCP agent (named client) wrote to the graph.
    AgentWrite(String),
    /// Manual "Sync now" / MCP `git_sync`.
    SyncNow,
    /// Network up / wake from sleep.
    NetworkUp,
    /// App focus changed.
    Focus(bool),
    /// Close the graph: commit, best-effort push, reply with the final state, stop.
    Close(Sender<SyncState>),
    /// Stop without syncing.
    Shutdown,
}

/// Handle to a running engine thread.
pub struct EngineHandle {
    tx: Sender<Command>,
    status: Arc<Mutex<SyncStatus>>,
    join: Option<JoinHandle<SyncEngine>>,
}

impl EngineHandle {
    /// Sends a command; ignored when the thread has stopped.
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    /// Latest published status.
    pub fn status(&self) -> SyncStatus {
        match self.status.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Stops the thread and returns the engine.
    pub fn shutdown(mut self) -> Option<SyncEngine> {
        let _ = self.tx.send(Command::Shutdown);
        self.join.take().and_then(|j| j.join().ok())
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        if let Some(j) = self.join.take() {
            let _ = self.tx.send(Command::Shutdown);
            let _ = j.join();
        }
    }
}

/// Runs `engine` on its own thread.
pub fn spawn(engine: SyncEngine) -> EngineHandle {
    let (tx, rx) = channel();
    let status = engine.shared_status();
    let join = std::thread::Builder::new()
        .name("bitacora-sync".to_string())
        .spawn(move || run_loop(engine, &rx))
        .ok();
    EngineHandle { tx, status, join }
}

fn run_loop(mut engine: SyncEngine, rx: &Receiver<Command>) -> SyncEngine {
    loop {
        let now = engine.timing.now();
        let wait = engine
            .next_deadline()
            .map_or(Duration::from_secs(3600), |d| {
                d.saturating_duration_since(now)
            });
        match rx.recv_timeout(wait) {
            Ok(Command::FileFlushed) => engine.note_write(),
            Ok(Command::AgentWrite(name)) => engine.note_agent_write(&name),
            Ok(Command::SyncNow) => {
                engine.sync_now();
            }
            Ok(Command::NetworkUp) => {
                engine.network_changed();
                engine.sync_now();
            }
            Ok(Command::Focus(f)) => engine.set_focused(f),
            Ok(Command::Close(reply)) => {
                let state = engine.close();
                let _ = reply.send(state);
                return engine;
            }
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => return engine,
            Err(RecvTimeoutError::Timeout) => engine.on_timer(),
        }
    }
}

impl std::fmt::Debug for SyncEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncEngine")
            .field("state", &self.state)
            .field("ahead", &self.ahead)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for EngineHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineHandle").finish_non_exhaustive()
    }
}
