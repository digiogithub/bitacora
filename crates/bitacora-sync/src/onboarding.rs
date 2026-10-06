//! Onboarding: enable sync on an existing graph folder, clone a graph from a remote, and migrate
//! Logseq's separate gitdir layout (design `git-sync-merge` 7, `05-git-and-apis` 1.1).
//!
//! History merging itself belongs to the sync engine: when the remote already has commits that the
//! local repository does not contain, [`enable_sync`] reports [`RemoteState::NeedsMerge`] and
//! pushes nothing, so the caller can run the block-aware merge (with an empty base when the
//! histories are unrelated, add/add policy).

use std::path::{Path, PathBuf};

use crate::backend::{
    CliBackend, CliConfig, CommitKind, CommitMessage, CommitOpts, GitBackend, GitDetection,
    GitError, GixBackend, Oid, select_backend,
};
use crate::repo_setup::{self, Identity, SetupError, cfg_err};

/// Errors from onboarding.
#[derive(Debug, thiserror::Error)]
pub enum OnboardingError {
    /// A git operation failed.
    #[error(transparent)]
    Git(#[from] GitError),
    /// Repository preparation failed.
    #[error(transparent)]
    Setup(#[from] SetupError),
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The graph uses Logseq's separate gitdir pointer; call [`migrate_separate_gitdir`] first.
    #[error("graph uses a separate gitdir at {0}; migration required")]
    SeparateGitdir(PathBuf),
    /// The existing `origin` points somewhere else.
    #[error("remote `{name}` already points to `{existing}`, not `{requested}`")]
    RemoteMismatch {
        /// Remote name.
        name: String,
        /// URL in the repository.
        existing: String,
        /// URL requested.
        requested: String,
    },
    /// Clone destination exists and is not empty.
    #[error("destination {0} is not empty")]
    DestinationNotEmpty(PathBuf),
    /// An argument looked unsafe (leading dash, whitespace, ...).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

type Result<T> = std::result::Result<T, OnboardingError>;

/// Relationship between local and remote history after connecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteState {
    /// The remote had no branch; the initial commit was pushed.
    EmptyRemotePushed,
    /// Local and remote already point at the same commit.
    InSync,
    /// Remote is an ancestor of local; local commits were pushed.
    LocalAheadPushed,
    /// Remote has history that must be merged before pushing (diverged, behind or unrelated).
    NeedsMerge {
        /// Remote tip (now at `refs/remotes/<remote>/<branch>`).
        remote_head: Oid,
        /// Local tip.
        local_head: Oid,
        /// Common ancestor; `None` means unrelated histories (merge with an empty base).
        merge_base: Option<Oid>,
    },
}

/// Result of [`enable_sync`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnableOutcome {
    /// Whether `git init` was run.
    pub initialized: bool,
    /// Whether an initial commit was created.
    pub committed: bool,
    /// Identity configured in the repository.
    pub identity: Identity,
    /// Where local and remote stand.
    pub remote: RemoteState,
}

/// Settings for onboarding operations.
#[derive(Debug, Clone)]
pub struct OnboardingConfig {
    /// Result of git detection.
    pub detection: GitDetection,
    /// CLI backend tunables.
    pub cli: CliConfig,
    /// Identity from settings; `None` uses repo-local values or the fallback.
    pub identity: Option<Identity>,
}

impl OnboardingConfig {
    /// Config with default CLI tunables and no explicit identity.
    pub fn new(detection: GitDetection) -> Self {
        Self {
            detection,
            cli: CliConfig::default(),
            identity: None,
        }
    }
}

const REMOTE: &str = "origin";

fn check_arg(what: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(|c| c.is_control()) {
        return Err(OnboardingError::InvalidArgument(format!(
            "{what}: `{value}`"
        )));
    }
    Ok(())
}

/// If `.git` is a pointer file (Logseq's `~/.logseq/git/...` layout), returns the gitdir it names.
pub fn detect_separate_gitdir(graph: &Path) -> Option<PathBuf> {
    let dot_git = graph.join(".git");
    if dot_git.is_file() {
        repo_setup::git_dir_of(graph)
    } else {
        None
    }
}

/// Copies the separate gitdir into `<graph>/.git`, drops `core.worktree`, and verifies the result.
/// The original gitdir is left untouched. Returns the new git directory.
pub fn migrate_separate_gitdir(graph: &Path) -> Result<PathBuf> {
    let Some(source) = detect_separate_gitdir(graph) else {
        return Ok(graph.join(".git"));
    };
    let staging = graph.join(".git.bitacora-migrating");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    copy_dir(&source, &staging)?;
    let result = (|| -> Result<()> {
        repo_setup::update_config(&staging, |file| {
            let mut changed = false;
            if let Ok(mut core) = file.section_mut("core", None) {
                changed = core.remove("worktree").is_some();
            }
            Ok(changed)
        })?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    // Swap: remove the pointer file, move the copy in place, then verify it opens.
    let dot_git = graph.join(".git");
    let backup = graph.join(".git.bitacora-pointer");
    std::fs::rename(&dot_git, &backup)?;
    if let Err(e) = std::fs::rename(&staging, &dot_git) {
        let _ = std::fs::rename(&backup, &dot_git);
        return Err(e.into());
    }
    match GixBackend::open(graph).and_then(|b| b.status().map(|_| ())) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup);
            Ok(dot_git)
        }
        Err(e) => {
            // Roll back to the pointer file.
            let _ = std::fs::remove_dir_all(&dot_git);
            let _ = std::fs::rename(&backup, &dot_git);
            Err(e.into())
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn init_repo(graph: &Path, branch: &str) -> Result<()> {
    gix::init(graph).map_err(GitError::other)?;
    std::fs::write(
        graph.join(".git").join("HEAD"),
        format!("ref: refs/heads/{branch}\n"),
    )?;
    Ok(())
}

fn ensure_remote(git_dir: &Path, name: &str, url: &str) -> Result<()> {
    let mut mismatch: Option<String> = None;
    repo_setup::update_config(git_dir, |file| {
        let key = format!("remote.{name}.url");
        if let Some(existing) = file.string(key.as_str()) {
            if existing != url {
                mismatch = Some(existing.to_string());
            }
            return Ok(false);
        }
        file.set_raw_value_by("remote", name, "url", url)
            .map_err(cfg_err)?;
        let spec = format!("+refs/heads/*:refs/remotes/{name}/*");
        file.set_raw_value_by("remote", name, "fetch", spec.as_str())
            .map_err(cfg_err)?;
        Ok(true)
    })?;
    if let Some(existing) = mismatch {
        return Err(OnboardingError::RemoteMismatch {
            name: name.to_string(),
            existing,
            requested: url.to_string(),
        });
    }
    Ok(())
}

fn set_upstream(git_dir: &Path, remote: &str, branch: &str) -> Result<()> {
    repo_setup::update_config(git_dir, |file| {
        file.set_raw_value_by("branch", branch, "remote", remote)
            .map_err(cfg_err)?;
        let merge = format!("refs/heads/{branch}");
        file.set_raw_value_by("branch", branch, "merge", merge.as_str())
            .map_err(cfg_err)?;
        Ok(true)
    })?;
    Ok(())
}

/// Connects `graph` to `remote_url`: `git init` when needed, set `origin`, prepare the repository,
/// make the initial commit and push (or report that a merge is needed).
pub fn enable_sync(
    graph: &Path,
    remote_url: &str,
    branch: &str,
    config: &OnboardingConfig,
) -> Result<EnableOutcome> {
    check_arg("remote url", remote_url)?;
    check_arg("branch", branch)?;
    if let Some(gitdir) = detect_separate_gitdir(graph) {
        return Err(OnboardingError::SeparateGitdir(gitdir));
    }
    let mut initialized = false;
    if !graph.join(".git").exists() {
        init_repo(graph, branch)?;
        initialized = true;
    }
    let git_dir = graph.join(".git");
    ensure_remote(&git_dir, REMOTE, remote_url)?;
    let identity = repo_setup::prepare_repo(graph, config.identity.as_ref())?;
    let backend = select_backend(&config.detection, graph, config.cli.clone())?;

    // Initial commit (Kind: migrate) when there is anything to record.
    let had_head = backend.status()?.head.is_some();
    let message = CommitMessage::new("bitacora: connect graph to sync")
        .trailer("Bitacora-Device", repo_setup::hostname())
        .kind(CommitKind::Migrate);
    let committed = match backend.commit(
        &message,
        CommitOpts {
            stage_all: true,
            ..CommitOpts::default()
        },
    ) {
        Ok(_) => true,
        Err(e) if had_head && is_nothing_to_commit(&e) => false,
        Err(e) => return Err(e.into()),
    };
    let local_head = backend
        .status()?
        .head
        .ok_or_else(|| GitError::other("no commit exists after initial commit"))?;

    let remote = match backend.fetch(REMOTE, branch) {
        Err(GitError::RemoteRefNotFound(_)) => {
            backend.push(REMOTE, branch)?;
            set_upstream(&git_dir, REMOTE, branch)?;
            RemoteState::EmptyRemotePushed
        }
        Err(e) => return Err(e.into()),
        Ok(fetched) => {
            let remote_head = fetched
                .remote_head
                .ok_or_else(|| GitError::other("fetch returned no remote head"))?;
            if remote_head == local_head {
                set_upstream(&git_dir, REMOTE, branch)?;
                RemoteState::InSync
            } else {
                let base = backend.merge_base(&local_head, &remote_head)?;
                if base.as_ref() == Some(&remote_head) {
                    backend.push(REMOTE, branch)?;
                    set_upstream(&git_dir, REMOTE, branch)?;
                    RemoteState::LocalAheadPushed
                } else {
                    RemoteState::NeedsMerge {
                        remote_head,
                        local_head,
                        merge_base: base,
                    }
                }
            }
        }
    };
    Ok(EnableOutcome {
        initialized,
        committed,
        identity,
        remote,
    })
}

fn is_nothing_to_commit(e: &GitError) -> bool {
    matches!(e, GitError::Other { stderr } if stderr.contains("nothing to commit")
        || stderr.contains("nothing added to commit")
        || stderr.contains("no changes added"))
}

/// Clones `url` into `dest` and prepares the repository (gitignore, attributes, identity).
pub fn clone_graph(url: &str, dest: &Path, config: &OnboardingConfig) -> Result<Identity> {
    check_arg("remote url", url)?;
    if dest.exists() && std::fs::read_dir(dest)?.next().is_some() {
        return Err(OnboardingError::DestinationNotEmpty(dest.to_path_buf()));
    }
    match &config.detection {
        GitDetection::Found { path, .. } => CliBackend::clone_repo(path, url, dest, &config.cli)?,
        GitDetection::Missing | GitDetection::TooOld { .. } => GixBackend::clone_repo(url, dest)?,
    }
    Ok(repo_setup::prepare_repo(dest, config.identity.as_ref())?)
}
