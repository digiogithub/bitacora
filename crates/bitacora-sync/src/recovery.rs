//! Startup recovery helpers (BIT-US-0047, BIT-SP-0006.R15, design `git-sync-merge` 2.6).
//!
//! The pure parts live here: stale `index.lock` handling and detection of operations another git
//! client left in progress. The engine's `recover` method combines them with the pending-merge
//! restore and the marker guard.

use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// A lock file older than this, with no git process running, is a leftover of a crash.
pub const STALE_LOCK_AGE: Duration = Duration::from_secs(10 * 60);

/// An operation another tool left half-done in the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalOperation {
    /// `git merge` / `git pull` stopped (`MERGE_HEAD`).
    Merge,
    /// `git rebase` in progress.
    Rebase,
    /// `git cherry-pick` in progress.
    CherryPick,
    /// `git revert` in progress.
    Revert,
}

impl ExternalOperation {
    /// Human name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }
}

/// Which external operation is in progress in `git_dir`, if any.
pub fn external_operation(git_dir: &Path) -> Option<ExternalOperation> {
    let has = |n: &str| git_dir.join(n).exists();
    if has("rebase-merge") || has("rebase-apply") {
        Some(ExternalOperation::Rebase)
    } else if has("CHERRY_PICK_HEAD") {
        Some(ExternalOperation::CherryPick)
    } else if has("REVERT_HEAD") {
        Some(ExternalOperation::Revert)
    } else if has("MERGE_HEAD") {
        Some(ExternalOperation::Merge)
    } else {
        None
    }
}

/// Whether a `git` process seems to be running (Linux: scans `/proc`; elsewhere unknown, so
/// `false` and only the lock age protects a live process).
pub fn git_process_running() -> bool {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return false;
    };
    dir.flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
            && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "git")
    })
}

/// Removes `<git_dir>/index.lock` when it is older than `min_age` at `now` and no git process
/// runs. Returns whether a file was removed.
///
/// # Errors
/// I/O errors other than the file being absent.
pub fn remove_stale_index_lock(
    git_dir: &Path,
    now: SystemTime,
    min_age: Duration,
    git_running: bool,
) -> io::Result<bool> {
    let lock = git_dir.join("index.lock");
    let meta = match std::fs::metadata(&lock) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if git_running {
        return Ok(false);
    }
    let age = meta
        .modified()
        .ok()
        .and_then(|m| now.duration_since(m).ok())
        .unwrap_or_default();
    if age < min_age {
        return Ok(false);
    }
    std::fs::remove_file(&lock)?;
    Ok(true)
}

/// What startup recovery found and did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// A stale `index.lock` was removed.
    pub stale_lock_removed: bool,
    /// Unresolved conflicts restored from `merge-state.json`.
    pub restored_conflicts: usize,
    /// A merge/rebase left by another tool (the engine is then in
    /// `Error(ExternalOperationInProgress)` until aborted or fixed).
    pub external_operation: Option<ExternalOperation>,
    /// A half-done `git pull` merge was taken over (its work tree is kept).
    pub took_over_pull: bool,
    /// Conflicts registered from files that contain markers written by another tool.
    pub marker_conflicts: usize,
    /// Work-tree changes found uncommitted (an interrupted session); an auto-commit is scheduled.
    pub uncommitted_paths: usize,
    /// Set when recovery itself failed (the message is user-facing).
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aged(dir: &Path, age: Duration) {
        let f = std::fs::File::create(dir.join("index.lock")).unwrap();
        f.set_modified(SystemTime::now() - age).unwrap();
    }

    #[test]
    fn stale_lock_removed_only_when_old_and_idle() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            !remove_stale_index_lock(d.path(), SystemTime::now(), STALE_LOCK_AGE, false).unwrap()
        );
        aged(d.path(), Duration::from_secs(30));
        assert!(
            !remove_stale_index_lock(d.path(), SystemTime::now(), STALE_LOCK_AGE, false).unwrap()
        );
        assert!(d.path().join("index.lock").exists());
        aged(d.path(), Duration::from_secs(3600));
        assert!(
            !remove_stale_index_lock(d.path(), SystemTime::now(), STALE_LOCK_AGE, true).unwrap()
        );
        assert!(
            remove_stale_index_lock(d.path(), SystemTime::now(), STALE_LOCK_AGE, false).unwrap()
        );
        assert!(!d.path().join("index.lock").exists());
    }

    #[test]
    fn detects_external_operations() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(external_operation(d.path()), None);
        std::fs::write(d.path().join("MERGE_HEAD"), "x").unwrap();
        assert_eq!(external_operation(d.path()), Some(ExternalOperation::Merge));
        std::fs::create_dir(d.path().join("rebase-merge")).unwrap();
        assert_eq!(
            external_operation(d.path()),
            Some(ExternalOperation::Rebase)
        );
    }
}
