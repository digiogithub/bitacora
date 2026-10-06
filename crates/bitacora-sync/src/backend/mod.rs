//! The `GitBackend` abstraction (ADR-007, ADR-020).
//!
//! Three implementations exist:
//! * [`CliBackend`]: the system `git` binary, used for network and ref-writing operations so SSH
//!   config, agents and credential helpers behave exactly as the user expects.
//! * [`GixBackend`]: in-process `gix` reads and tree building; it also implements every network and
//!   ref-writing operation so it can run alone when no usable system git exists.
//! * [`HybridBackend`]: delegates network/ref writes to the CLI and reads/tree building to gix.

mod cli;
mod detect;
mod fake;
mod gix_net;
mod gix_read;
mod gix_trees;
mod hybrid;

use std::fmt;
use std::path::Path;

pub use cli::{CliBackend, CliConfig};
pub use detect::{GitDetection, GitVersion, MIN_GIT_VERSION, detect_git, parse_git_version};
pub use fake::{FakeBackend, FakeCall};
pub use gix_net::GixBackend;
pub use hybrid::HybridBackend;

/// Result alias for backend operations.
pub type Result<T> = std::result::Result<T, GitError>;

/// Repository-relative path using `/` separators.
pub type RepoPath = String;

/// A SHA-1 object id as 40 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Oid(String);

impl Oid {
    /// Parses a 40-digit hex id.
    pub fn from_hex(hex: &str) -> Result<Self> {
        let hex = hex.trim();
        if hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self(hex.to_ascii_lowercase()))
        } else {
            Err(GitError::other(format!("invalid object id `{hex}`")))
        }
    }

    /// The hex representation.
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which backend is active; exposed in sync status and `doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveBackend {
    /// System git for network/ref writes plus gix for reads (ADR-007).
    Hybrid,
    /// Pure-Rust `gix` for everything (ADR-020 fallback).
    GixOnly,
}

/// Errors from git operations, classified from exit codes and stderr.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The remote could not be reached.
    #[error("network error: {0}")]
    Network(String),
    /// Authentication or authorisation failed.
    #[error("authentication failed{}", .hint.as_deref().map(|h| format!(" ({h})")).unwrap_or_default())]
    Auth {
        /// Human-readable suggestion (for example "install git").
        hint: Option<String>,
    },
    /// The push was rejected because the remote has commits we lack.
    #[error("push rejected: not a fast-forward")]
    NonFastForward,
    /// The remote has no such branch (typically an empty remote).
    #[error("remote ref not found: {0}")]
    RemoteRefNotFound(String),
    /// The system git is older than [`MIN_GIT_VERSION`].
    #[error("git {found} is older than the required {MIN_GIT_VERSION}")]
    GitTooOld {
        /// The version that was found.
        found: String,
    },
    /// The directory is not a git repository.
    #[error("not a git repository")]
    NotARepo,
    /// Another git operation (merge, rebase, lock file) is in progress.
    #[error("another git operation is in progress")]
    ExternalOperationInProgress,
    /// A compare-and-swap ref update found a different current value.
    #[error("ref `{0}` changed concurrently")]
    RefChanged(String),
    /// The operation is not supported by this backend.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Spawning or talking to the git process failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Anything else, with captured stderr.
    #[error("git failed: {stderr}")]
    Other {
        /// Captured stderr or a description.
        stderr: String,
    },
}

impl GitError {
    pub(crate) fn other(msg: impl fmt::Display) -> Self {
        Self::Other {
            stderr: msg.to_string(),
        }
    }
}

/// Classifies a failed git invocation from its stderr (exit code is non-zero by construction).
pub fn classify_failure(stderr: &str) -> GitError {
    let s = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| s.contains(n));
    if has(&["not a git repository"]) {
        GitError::NotARepo
    } else if has(&[
        "non-fast-forward",
        "[rejected]",
        "fetch first",
        "updates were rejected",
    ]) {
        GitError::NonFastForward
    } else if has(&[
        "authentication failed",
        "permission denied",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "invalid credentials",
        "the requested url returned error: 401",
        "the requested url returned error: 403",
        "host key verification failed",
    ]) {
        GitError::Auth { hint: None }
    } else if has(&[
        "couldn't find remote ref",
        "could not find remote branch",
        "matched any of the",
    ]) {
        GitError::RemoteRefNotFound(stderr.trim().to_string())
    } else if has(&[
        "could not resolve host",
        "connection refused",
        "connection timed out",
        "network is unreachable",
        "unable to access",
        "operation timed out",
        "could not read from remote repository",
        "failed to connect",
        "temporary failure in name resolution",
        "timed out after",
    ]) {
        GitError::Network(stderr.trim().to_string())
    } else if has(&[
        ".lock': file exists",
        "another git process",
        "you are in the middle of",
        "merge_head exists",
    ]) {
        GitError::ExternalOperationInProgress
    } else {
        GitError::Other {
            stderr: stderr.trim().to_string(),
        }
    }
}

/// Result of a fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    /// Remote-tracking tip after the fetch (`None` when the remote branch does not exist).
    pub remote_head: Option<Oid>,
    /// Whether the remote-tracking ref moved.
    pub updated: bool,
}

/// Result of a push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    /// `false` when the remote already had our commits.
    pub pushed: bool,
}

/// Commit kind recorded in the `Bitacora-Kind` trailer (design 2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    /// Automatic commit after idle.
    Auto,
    /// User-initiated commit.
    Manual,
    /// Merge commit.
    Merge,
    /// Conflict resolution.
    Resolve,
    /// Write from an MCP agent.
    Agent,
    /// Onboarding / migration.
    Migrate,
}

impl CommitKind {
    /// Trailer value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Merge => "merge",
            Self::Resolve => "resolve",
            Self::Agent => "agent",
            Self::Migrate => "migrate",
        }
    }
}

/// A commit message with machine-parseable trailers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMessage {
    /// First line (callers keep it at most 72 chars).
    pub subject: String,
    /// `Key: value` trailers, rendered in order after a blank line.
    pub trailers: Vec<(String, String)>,
}

impl CommitMessage {
    /// A message with only a subject.
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            trailers: Vec::new(),
        }
    }

    /// Adds a trailer.
    pub fn trailer(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.trailers.push((key.into(), value.into()));
        self
    }

    /// Adds the `Bitacora-Kind` trailer.
    pub fn kind(self, kind: CommitKind) -> Self {
        self.trailer("Bitacora-Kind", kind.as_str())
    }

    /// Full message text, ending in a newline.
    pub fn render(&self) -> String {
        let mut out = self.subject.clone();
        out.push('\n');
        if !self.trailers.is_empty() {
            out.push('\n');
            for (k, v) in &self.trailers {
                out.push_str(&format!("{k}: {v}\n"));
            }
        }
        out
    }
}

/// Options for [`GitBackend::commit`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CommitOpts {
    /// Stage every change in the work tree (`add -A`) before committing.
    pub stage_all: bool,
    /// Replace the previous commit instead of creating a new one (squash of unpushed auto commits).
    pub amend: bool,
    /// Create the commit even if the tree is unchanged.
    pub allow_empty: bool,
}

/// One change between two trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeChange {
    /// File added in `b`.
    Added {
        /// Path.
        path: RepoPath,
    },
    /// File removed in `b`.
    Deleted {
        /// Path.
        path: RepoPath,
    },
    /// Content changed.
    Modified {
        /// Path.
        path: RepoPath,
    },
    /// File moved (and possibly edited).
    Renamed {
        /// Old path.
        from: RepoPath,
        /// New path.
        to: RepoPath,
        /// Similarity in percent (0 to 100).
        similarity: u8,
    },
}

/// An edit applied by [`GitBackend::write_tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeEdit {
    /// Create or replace a regular file.
    Upsert {
        /// Path.
        path: RepoPath,
        /// New content.
        content: Vec<u8>,
    },
    /// Remove a file.
    Remove {
        /// Path.
        path: RepoPath,
    },
}

/// A work-tree path that differs from the index/HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyPath {
    /// Path.
    pub path: RepoPath,
    /// What kind of change.
    pub kind: DirtyKind,
}

/// Kind of work-tree change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyKind {
    /// Not tracked yet.
    Untracked,
    /// Tracked and modified.
    Modified,
    /// Tracked and removed from the work tree.
    Deleted,
}

/// An unmerged index entry with its three stages (base, ours, theirs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmergedEntry {
    /// Path.
    pub path: RepoPath,
    /// Stage 1 (common ancestor).
    pub base: Option<Oid>,
    /// Stage 2 (ours).
    pub ours: Option<Oid>,
    /// Stage 3 (theirs).
    pub theirs: Option<Oid>,
}

/// Repository status snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoStatus {
    /// Current `HEAD` commit, `None` on an unborn branch.
    pub head: Option<Oid>,
    /// Current branch short name, `None` when detached.
    pub branch: Option<String>,
    /// Dirty work-tree paths, sorted.
    pub dirty: Vec<DirtyPath>,
    /// Unmerged entries, sorted.
    pub unmerged: Vec<UnmergedEntry>,
}

/// Git operations the sync engine needs.
pub trait GitBackend: Send + Sync {
    /// Fetches `branch` from `remote` into `refs/remotes/<remote>/<branch>` (pruning, no tags).
    fn fetch(&self, remote: &str, branch: &str) -> Result<FetchOutcome>;
    /// Pushes `HEAD` to `refs/heads/<branch>` on `remote`; never forced.
    fn push(&self, remote: &str, branch: &str) -> Result<PushOutcome>;
    /// Merge base of two commits, if any.
    fn merge_base(&self, a: &Oid, b: &Oid) -> Result<Option<Oid>>;
    /// Blob content at `path` in `commit`, `None` when absent.
    fn read_blob(&self, commit: &Oid, path: &str) -> Result<Option<Vec<u8>>>;
    /// Differences between two commits or trees, with rename detection.
    fn diff_trees(&self, a: &Oid, b: &Oid) -> Result<Vec<TreeChange>>;
    /// Builds a new tree from `base` (a tree or commit id) plus `edits` without touching the work
    /// tree or index.
    fn write_tree(&self, base: &Oid, edits: &[TreeEdit]) -> Result<Oid>;
    /// Commits the index (optionally staging everything first) and moves `HEAD`.
    fn commit(&self, msg: &CommitMessage, opts: CommitOpts) -> Result<Oid>;
    /// Creates a commit object for `tree` with `parents`, without moving any ref.
    fn commit_tree(&self, tree: &Oid, parents: &[Oid], msg: &CommitMessage) -> Result<Oid>;
    /// Compare-and-swap ref update; `expected_old = None` requires the ref to be absent.
    fn update_ref(&self, name: &str, new: &Oid, expected_old: Option<&Oid>) -> Result<()>;
    /// Work-tree and index status.
    fn status(&self) -> Result<RepoStatus>;
    /// Which backend this is.
    fn kind(&self) -> ActiveBackend;
}

/// Validates a branch or remote name before it reaches a command line.
pub(crate) fn check_ref_arg(arg: &str) -> Result<()> {
    if arg.is_empty()
        || arg.starts_with('-')
        || arg.contains(|c: char| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
        || arg.contains("..")
    {
        return Err(GitError::other(format!("invalid ref argument `{arg}`")));
    }
    Ok(())
}

/// Decides the backend kind from detection without constructing anything.
pub fn backend_kind_for(detection: &GitDetection) -> ActiveBackend {
    match detection {
        GitDetection::Found { .. } => ActiveBackend::Hybrid,
        GitDetection::Missing | GitDetection::TooOld { .. } => ActiveBackend::GixOnly,
    }
}

/// Builds the backend for the repository at `repo` according to detection (ADR-020).
///
/// A found git of at least [`MIN_GIT_VERSION`] selects the hybrid backend; anything else selects
/// the gix-only backend.
pub fn select_backend(
    detection: &GitDetection,
    repo: &Path,
    config: CliConfig,
) -> Result<Box<dyn GitBackend>> {
    match detection {
        GitDetection::Found { path, .. } => {
            let cli = CliBackend::new(repo, path.clone(), config);
            let gix = GixBackend::open(repo)?;
            Ok(Box::new(HybridBackend::new(cli, gix)))
        }
        GitDetection::Missing | GitDetection::TooOld { .. } => {
            Ok(Box::new(GixBackend::open(repo)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_variants() {
        assert!(matches!(
            classify_failure("fatal: not a git repository (or any parent)"),
            GitError::NotARepo
        ));
        assert!(matches!(
            classify_failure(" ! [rejected]        HEAD -> main (non-fast-forward)"),
            GitError::NonFastForward
        ));
        assert!(matches!(
            classify_failure(
                "fatal: could not read Username for 'https://x': terminal prompts disabled"
            ),
            GitError::Auth { .. }
        ));
        assert!(matches!(
            classify_failure(
                "git@host: Permission denied (publickey).\nfatal: Could not read from remote repository."
            ),
            GitError::Auth { .. }
        ));
        assert!(matches!(
            classify_failure("fatal: unable to access 'https://x/': Could not resolve host: x"),
            GitError::Network(_)
        ));
        assert!(matches!(
            classify_failure("fatal: couldn't find remote ref main"),
            GitError::RemoteRefNotFound(_)
        ));
        assert!(matches!(
            classify_failure("fatal: Unable to create '.git/index.lock': File exists."),
            GitError::ExternalOperationInProgress
        ));
        assert!(matches!(classify_failure("boom"), GitError::Other { .. }));
    }

    #[test]
    fn message_render() {
        let m = CommitMessage::new("bitacora: edit 1 page")
            .trailer("Bitacora-Device", "laptop")
            .kind(CommitKind::Auto);
        assert_eq!(
            m.render(),
            "bitacora: edit 1 page\n\nBitacora-Device: laptop\nBitacora-Kind: auto\n"
        );
    }

    #[test]
    fn ref_arg_validation() {
        assert!(check_ref_arg("main").is_ok());
        assert!(check_ref_arg("feature/x").is_ok());
        assert!(check_ref_arg("--upload-pack=x").is_err());
        assert!(check_ref_arg("a b").is_err());
        assert!(check_ref_arg("").is_err());
    }

    #[test]
    fn oid_parsing() {
        assert!(Oid::from_hex("zz").is_err());
        assert!(Oid::from_hex(&"A".repeat(40)).is_ok());
    }
}
