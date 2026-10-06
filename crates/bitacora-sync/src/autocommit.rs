//! Idle-debounced auto-commit with squashing of unpushed auto commits (BIT-SP-0006.R1/R2,
//! design `git-sync-merge` 2.1 and 2.4).
//!
//! * [`Debouncer`]: pure timing (idle window plus hard cap), driven by the caller's clock.
//! * [`commit_changes`]: one commit step; the caller must hold the graph lock.
//! * [`commit_locked`]: acquires the lock from a [`GraphWriter`] first.

use std::time::{Duration, Instant};

use crate::backend::{CommitKind, CommitOpts, GitBackend, GitError, Oid};
use crate::commit_msg::{MessageSpec, build_message, parse_message};
use crate::writer::{GraphWriter, WriterError};

/// Smallest accepted idle window.
pub const MIN_IDLE: Duration = Duration::from_secs(5);
/// Largest accepted idle window.
pub const MAX_IDLE: Duration = Duration::from_secs(600);

/// Auto-commit settings (`sync.*`, design 7).
#[derive(Debug, Clone)]
pub struct CommitSettings {
    /// `Bitacora-Device` value.
    pub device: String,
    /// Remote used to decide whether a commit was pushed.
    pub remote: String,
    /// Branch used to decide whether a commit was pushed.
    pub branch: String,
    /// `sync.squash_auto_commits`.
    pub squash: bool,
    /// Squash window: only commits younger than this are amended.
    pub squash_window: Duration,
    /// `sync.commit_idle_secs` (clamped to [`MIN_IDLE`], [`MAX_IDLE`]).
    pub idle: Duration,
    /// `sync.commit_max_secs`: hard cap of continuous editing.
    pub max: Duration,
}

impl CommitSettings {
    /// Defaults of the design: idle 20 s, cap 300 s, squash within 30 min.
    pub fn new(device: impl Into<String>, remote: &str, branch: &str) -> Self {
        Self {
            device: device.into(),
            remote: remote.to_string(),
            branch: branch.to_string(),
            squash: true,
            squash_window: Duration::from_secs(30 * 60),
            idle: Duration::from_secs(20),
            max: Duration::from_secs(300),
        }
    }
}

/// Idle debouncer with a hard cap. Pure: every method takes the current time.
#[derive(Debug, Clone)]
pub struct Debouncer {
    idle: Duration,
    max: Duration,
    first_write: Option<Instant>,
    last_write: Option<Instant>,
}

impl Debouncer {
    /// Creates a debouncer; `idle` is clamped to [`MIN_IDLE`]..=[`MAX_IDLE`] and `max` is at
    /// least `idle`.
    pub fn new(idle: Duration, max: Duration) -> Self {
        Self::with_floor(idle, max, MIN_IDLE)
    }

    /// Like [`Debouncer::new`] with a custom lower bound for `idle` (tests use a tiny floor).
    pub fn with_floor(idle: Duration, max: Duration, floor: Duration) -> Self {
        let idle = idle.clamp(floor.min(MAX_IDLE), MAX_IDLE);
        Self {
            idle,
            max: max.max(idle),
            first_write: None,
            last_write: None,
        }
    }

    /// Records a flushed write (editor, MCP, watcher). Resets the idle window; the cap keeps
    /// counting from the first write of the burst.
    pub fn note_write(&mut self, now: Instant) {
        self.first_write.get_or_insert(now);
        self.last_write = Some(now);
    }

    /// `true` while uncommitted writes are pending.
    pub fn is_dirty(&self) -> bool {
        self.last_write.is_some()
    }

    /// When the commit becomes due: the earlier of idle expiry and the hard cap.
    pub fn deadline(&self) -> Option<Instant> {
        let idle_at = self.last_write? + self.idle;
        let cap_at = self.first_write? + self.max;
        Some(idle_at.min(cap_at))
    }

    /// `true` when a commit is due at `now`.
    pub fn is_due(&self, now: Instant) -> bool {
        self.deadline().is_some_and(|d| now >= d)
    }

    /// Forgets pending writes (after a commit attempt).
    pub fn clear(&mut self) {
        self.first_write = None;
        self.last_write = None;
    }
}

/// `true` for paths whose changes never justify a commit (ignored or volatile files).
pub fn is_ignored_path(path: &str) -> bool {
    const PREFIXES: [&str; 5] = [
        "logseq/bak/",
        "logseq/.recycle/",
        "logseq/version-files/",
        "logseq/.bitacora/",
        ".trash/",
    ];
    if PREFIXES.iter().any(|p| path.starts_with(p)) {
        return true;
    }
    if matches!(path, "logseq/graphs-txid.edn" | "logseq/pages-metadata.edn") {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with('~') || matches!(name, ".DS_Store" | "Thumbs.db")
}

/// What a commit step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    /// A commit was created or amended.
    Committed {
        /// The new HEAD.
        oid: Oid,
        /// Whether the previous commit was replaced.
        amended: bool,
        /// Paths recorded in the message.
        pages: Vec<String>,
    },
    /// Nothing to do.
    Skipped(SkipReason),
}

/// Why no commit was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The work tree matches HEAD.
    Clean,
    /// Only ignored or volatile files changed.
    OnlyIgnored,
}

/// Errors of a commit step.
#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    /// A git operation failed.
    #[error(transparent)]
    Git(#[from] GitError),
    /// The graph writer refused the lock.
    #[error(transparent)]
    Writer(#[from] WriterError),
}

/// What to commit.
#[derive(Debug, Clone)]
pub struct CommitRequest {
    /// Kind recorded in the trailer. Only [`CommitKind::Auto`] is ever squashed.
    pub kind: CommitKind,
    /// MCP client name for [`CommitKind::Agent`].
    pub agent: Option<String>,
    /// Subject override (manual commits).
    pub subject: Option<String>,
}

impl CommitRequest {
    /// An automatic commit.
    pub fn auto() -> Self {
        Self {
            kind: CommitKind::Auto,
            agent: None,
            subject: None,
        }
    }
}

/// `true` when `head` is not reachable from the remote-tracking branch (never pushed).
pub fn is_unpushed(
    backend: &dyn GitBackend,
    settings: &CommitSettings,
    head: &Oid,
) -> Result<bool, GitError> {
    let tracking = format!("refs/remotes/{}/{}", settings.remote, settings.branch);
    let Some(remote) = backend.resolve_ref(&tracking)? else {
        return Ok(true);
    };
    if &remote == head {
        return Ok(false);
    }
    Ok(backend.merge_base(head, &remote)?.as_ref() != Some(head))
}

/// Decides whether HEAD can be amended instead of adding a new commit, and returns the pages
/// already recorded in it. Conditions: `Kind: auto`, this device, unpushed, younger than the
/// squash window.
fn squash_target(
    backend: &dyn GitBackend,
    settings: &CommitSettings,
    head: &Oid,
    now_unix: i64,
) -> Result<Option<Vec<String>>, GitError> {
    let info = backend.commit_info(head)?;
    let parsed = parse_message(&info.message);
    if parsed.kind != Some(CommitKind::Auto) || parsed.device.as_deref() != Some(&settings.device) {
        return Ok(None);
    }
    let age = now_unix.saturating_sub(info.committer_time);
    if age < 0 || age as u64 >= settings.squash_window.as_secs() {
        return Ok(None);
    }
    if !is_unpushed(backend, settings, head)? {
        return Ok(None);
    }
    Ok(Some(parsed.pages))
}

/// One commit step. The caller MUST hold the graph lock (staging reads the work tree). Stages
/// everything, builds the structured message, and amends HEAD when the squash rules allow it.
pub fn commit_changes(
    backend: &dyn GitBackend,
    settings: &CommitSettings,
    request: &CommitRequest,
    now_unix: i64,
) -> Result<CommitOutcome, GitError> {
    let status = backend.status()?;
    if !status.unmerged.is_empty() {
        return Err(GitError::ExternalOperationInProgress);
    }
    if status.dirty.is_empty() {
        return Ok(CommitOutcome::Skipped(SkipReason::Clean));
    }
    let mut pages: Vec<String> = status
        .dirty
        .iter()
        .filter(|d| !is_ignored_path(&d.path))
        .map(|d| d.path.clone())
        .collect();
    if pages.is_empty() {
        return Ok(CommitOutcome::Skipped(SkipReason::OnlyIgnored));
    }
    pages.sort();

    let mut amend = false;
    if settings.squash
        && request.kind == CommitKind::Auto
        && let Some(head) = &status.head
        && let Some(previous) = squash_target(backend, settings, head, now_unix)?
    {
        amend = true;
        let mut union = previous;
        for p in pages {
            if !union.contains(&p) {
                union.push(p);
            }
        }
        pages = union;
    }
    let message = build_message(&MessageSpec {
        kind: request.kind,
        device: &settings.device,
        pages: &pages,
        agent: request.agent.as_deref(),
        subject: request.subject.as_deref(),
    });
    let opts = CommitOpts {
        stage_all: true,
        amend,
        allow_empty: false,
    };
    match backend.commit(&message, opts) {
        Ok(oid) => Ok(CommitOutcome::Committed {
            oid,
            amended: amend,
            pages,
        }),
        Err(e) if crate::onboarding::is_nothing_to_commit(&e) => {
            Ok(CommitOutcome::Skipped(SkipReason::Clean))
        }
        Err(e) => Err(e),
    }
}

/// Acquires the graph lock (flushing editors) and runs [`commit_changes`] under it.
pub fn commit_locked(
    backend: &dyn GitBackend,
    writer: &dyn GraphWriter,
    settings: &CommitSettings,
    request: &CommitRequest,
    now_unix: i64,
) -> Result<CommitOutcome, CommitError> {
    let _lock = writer.acquire()?;
    Ok(commit_changes(backend, settings, request, now_unix)?)
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Instant {
        Instant::now()
    }

    #[test]
    fn idle_commit_fires_after_idle_window() {
        let t0 = base();
        let mut d = Debouncer::new(Duration::from_secs(20), Duration::from_secs(300));
        assert!(!d.is_dirty() && d.deadline().is_none());
        d.note_write(t0);
        assert!(!d.is_due(t0 + Duration::from_secs(19)));
        assert!(d.is_due(t0 + Duration::from_secs(20)));
    }

    #[test]
    fn writes_reset_the_idle_window_until_the_cap() {
        let t0 = base();
        let mut d = Debouncer::new(Duration::from_secs(20), Duration::from_secs(300));
        // A write every 10 s for 6 minutes: idle never expires, the cap does at t=300.
        let mut t = 0;
        let mut fired_at = None;
        while t <= 360 {
            let now = t0 + Duration::from_secs(t);
            if d.is_due(now) {
                fired_at = Some(t);
                break;
            }
            d.note_write(now);
            t += 10;
        }
        assert_eq!(fired_at, Some(300));
        d.clear();
        assert!(!d.is_dirty());
    }

    #[test]
    fn idle_is_clamped() {
        let t0 = base();
        let mut d = Debouncer::new(Duration::from_secs(1), Duration::from_secs(2));
        d.note_write(t0);
        assert_eq!(d.deadline(), Some(t0 + MIN_IDLE));
        let mut d = Debouncer::new(Duration::from_secs(10_000), Duration::from_secs(20_000));
        d.note_write(t0);
        assert_eq!(d.deadline(), Some(t0 + MAX_IDLE));
    }

    #[test]
    fn ignored_paths() {
        for p in [
            "logseq/bak/pages/Ideas/2026-10-06.md",
            "logseq/.recycle/x.md",
            "logseq/version-files/a",
            "logseq/.bitacora/index.sqlite",
            ".DS_Store",
            "pages/.DS_Store",
            "pages/A.md~",
            "logseq/graphs-txid.edn",
            "logseq/pages-metadata.edn",
        ] {
            assert!(is_ignored_path(p), "{p}");
        }
        for p in [
            "pages/A.md",
            "journals/2026_10_06.md",
            "logseq/config.edn",
            "assets/a.png",
        ] {
            assert!(!is_ignored_path(p), "{p}");
        }
    }
}
