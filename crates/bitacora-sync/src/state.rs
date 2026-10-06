//! Sync states, the transition table of design `git-sync-merge` 2.5, status snapshots, back-off
//! and the pending-merge seam used until the persisted conflict store lands (BIT-US-0053).

use std::time::{Duration, SystemTime};

use crate::backend::{ActiveBackend, Oid};
use crate::merge::ConflictRecord;

/// Why the engine stopped in [`SyncState::Error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// The push was rejected as non-fast-forward on every attempt.
    PushRejectedLoop,
    /// Authentication failed.
    Auth(String),
    /// The user (or another tool) left a merge/rebase in progress in the repository.
    ExternalOperationInProgress,
    /// The graph writer failed.
    Writer(String),
    /// Any other git failure.
    Git(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PushRejectedLoop => f.write_str("push rejected repeatedly (remote keeps moving)"),
            Self::Auth(m) => write!(f, "authentication failed: {m}"),
            Self::ExternalOperationInProgress => {
                f.write_str("a git merge or rebase is in progress in the graph repository")
            }
            Self::Writer(m) => write!(f, "graph writer failed: {m}"),
            Self::Git(m) => write!(f, "git failed: {m}"),
        }
    }
}

/// The states of the sync machine (design 2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    /// Sync is off (no remote configured or the user disabled it).
    Disabled,
    /// Nothing to do.
    Idle,
    /// Local edits are waiting for the debounce.
    Dirty,
    /// A commit step is running.
    Committing,
    /// A sync cycle started.
    Syncing,
    /// Fetching the remote branch.
    Fetching,
    /// Deciding how to integrate remote work.
    Integrating,
    /// Fast-forwarding to the remote tip.
    FastForward,
    /// Running the block-aware merge.
    Merging,
    /// A merge has unresolved content conflicts.
    Conflicted,
    /// Pushing.
    Pushing,
    /// The remote is unreachable; local commits continue.
    Offline,
    /// Needs attention.
    Error(SyncError),
}

impl SyncState {
    /// Whether the diagram of design 2.5 (plus the documented extra edges) allows `self -> to`.
    ///
    /// Extra edges beyond the diagram: `Committing -> Dirty` (the writer was busy), anything ->
    /// `Error`/`Disabled`, `Conflicted -> Committing/Fetching/Syncing` (local edits and periodic
    /// fetch while conflicted), `Fetching -> Conflicted` (remote did not move), `Pushing ->
    /// Integrating` (rejected push re-integrates) and `Offline -> Fetching`.
    pub fn can_transition(&self, to: &Self) -> bool {
        use SyncState::{
            Committing, Conflicted, Dirty, Disabled, Error, FastForward, Fetching, Idle,
            Integrating, Merging, Offline, Pushing, Syncing,
        };
        // Interrupts: errors, disabling, a deferral back to `Dirty` and a pending merge.
        if matches!(to, Error(_) | Disabled | Dirty | Conflicted) || self == to {
            return true;
        }
        matches!(
            (self, to),
            (Disabled, Idle)
                | (Idle, Dirty | Committing | Syncing)
                | (Dirty, Committing)
                | (Committing, Idle | Syncing | Offline)
                | (Syncing, Fetching)
                | (Fetching, Offline | Integrating | Pushing | Idle)
                | (Integrating, FastForward | Merging | Fetching)
                | (FastForward, Idle | Fetching)
                | (Merging, Pushing | Fetching)
                | (Conflicted, Committing | Fetching | Syncing | Pushing | Idle)
                | (Pushing, Idle | Fetching | Offline | Integrating)
                | (Offline, Syncing | Committing | Fetching | Idle)
                | (Error(_), Syncing | Committing | Idle)
        )
    }
}

/// Snapshot published to the UI status bar and MCP `git_sync_status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatus {
    /// Current state.
    pub state: SyncState,
    /// Local commits not on the remote ("N local commits not synced").
    pub ahead: usize,
    /// When the last full sync (fetch + integrate + push) finished.
    pub last_sync: Option<SystemTime>,
    /// Number of unresolved conflicts.
    pub conflicts: usize,
    /// Last error or offline reason.
    pub last_error: Option<String>,
    /// Active backend.
    pub backend: ActiveBackend,
}

/// Offline back-off schedule: 30 s, 1 m, 2 m, 5 m, 10 m (cap); reset on network change.
#[derive(Debug, Clone)]
pub struct Backoff {
    steps: Vec<Duration>,
    index: usize,
}

impl Backoff {
    /// The schedule of the design.
    pub fn standard() -> Self {
        Self::new(
            [30, 60, 120, 300, 600]
                .into_iter()
                .map(Duration::from_secs)
                .collect(),
        )
    }

    /// A custom schedule (last step repeats); must not be empty.
    pub fn new(steps: Vec<Duration>) -> Self {
        let steps = if steps.is_empty() {
            vec![Duration::from_secs(30)]
        } else {
            steps
        };
        Self { steps, index: 0 }
    }

    /// The delay before the next retry; advances the schedule.
    pub fn next_delay(&mut self) -> Duration {
        let d = self.steps[self.index.min(self.steps.len() - 1)];
        self.index = self.index.saturating_add(1);
        d
    }

    /// Resets to the first step.
    pub fn reset(&mut self) {
        self.index = 0;
    }
}

/// A merge waiting for the user (persisted by BIT-US-0053; the engine only needs this seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMerge {
    /// Common ancestor (`None`: unrelated histories).
    pub base: Option<Oid>,
    /// Our tip when the merge was computed.
    pub ours: Oid,
    /// Their tip (the remote commit being merged).
    pub theirs: Oid,
    /// Commit holding the merged tree with parents `[ours, theirs]`, stored in
    /// `refs/bitacora/pending-merge` so their commits stay reachable.
    pub pending_commit: Oid,
    /// Unresolved conflicts.
    pub conflicts: Vec<ConflictRecord>,
}

/// Storage for the pending merge. The in-memory implementation is enough for the engine; the
/// persisted store (`.git/bitacora/merge-state.json`, resolution memo) implements this trait.
pub trait MergeStateStore: Send {
    /// The pending merge, if any (recovered on start).
    fn load(&self) -> Option<PendingMerge>;
    /// Replaces the pending merge.
    fn save(&mut self, pending: &PendingMerge);
    /// Forgets the pending merge.
    fn clear(&mut self);
}

/// In-memory [`MergeStateStore`].
#[derive(Debug, Default)]
pub struct MemoryMergeStore(Option<PendingMerge>);

impl MemoryMergeStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MergeStateStore for MemoryMergeStore {
    fn load(&self) -> Option<PendingMerge> {
        self.0.clone()
    }
    fn save(&mut self, pending: &PendingMerge) {
        self.0 = Some(pending.clone());
    }
    fn clear(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_schedule_and_reset() {
        let mut b = Backoff::standard();
        let got: Vec<u64> = (0..7).map(|_| b.next_delay().as_secs()).collect();
        assert_eq!(got, [30, 60, 120, 300, 600, 600, 600]);
        b.reset();
        assert_eq!(b.next_delay().as_secs(), 30);
    }

    #[test]
    fn diagram_edges() {
        use SyncState::*;
        assert!(Idle.can_transition(&Dirty));
        assert!(Dirty.can_transition(&Committing));
        assert!(Committing.can_transition(&Syncing));
        assert!(Fetching.can_transition(&Integrating));
        assert!(Integrating.can_transition(&Merging));
        assert!(Merging.can_transition(&Conflicted));
        assert!(Pushing.can_transition(&Fetching));
        assert!(Offline.can_transition(&Syncing));
        assert!(Error(SyncError::PushRejectedLoop).can_transition(&Syncing));
        // Not in the diagram.
        assert!(!Idle.can_transition(&Merging));
        assert!(!Dirty.can_transition(&Pushing));
        assert!(!Idle.can_transition(&Offline));
        assert!(!FastForward.can_transition(&Pushing));
        assert!(!Merging.can_transition(&FastForward));
    }
}
