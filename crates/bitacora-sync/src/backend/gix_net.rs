//! `GixBackend`: the pure-Rust backend (ADR-020) and the network / ref-writing half implemented
//! on gix.
//!
//! Known limitation (gix 0.88 has no push client): `push` works for local (`file://` or plain
//! path) remotes by copying objects and compare-and-swapping the remote ref; pushing over
//! HTTPS/SSH returns [`GitError::Unsupported`] with a hint to install git.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gix::objs::Write as _;
use gix::refs::Target;
use gix::refs::transaction::PreviousValue;

use super::gix_read::{from_gix, to_gix};
use super::{
    ActiveBackend, CommitInfo, CommitMessage, CommitOpts, DirtyKind, FetchOutcome, GitBackend,
    GitError, Oid, PushOutcome, RepoStatus, Result, TreeChange, TreeEdit, check_ref_arg,
    classify_failure, gix_read, gix_trees,
};

/// Backend running entirely on gix.
#[derive(Debug, Clone)]
pub struct GixBackend {
    path: PathBuf,
}

/// Flattens an error and its sources into one string and classifies it.
fn net_err(e: &(dyn std::error::Error + 'static)) -> GitError {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    match classify_failure(&msg) {
        GitError::Auth { .. } => GitError::Auth {
            hint: Some("install git for full credential support".into()),
        },
        other => other,
    }
}

impl GixBackend {
    /// Opens the repository whose work tree (or git dir) is `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let backend = Self {
            path: path.to_path_buf(),
        };
        backend.repo()?;
        Ok(backend)
    }

    fn repo(&self) -> Result<gix::Repository> {
        gix::open(&self.path).map_err(|_| GitError::NotARepo)
    }

    /// Clones `url` into `dest` and checks out the default branch.
    pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
        let interrupt = AtomicBool::new(false);
        let mut prepare = gix::prepare_clone(url, dest).map_err(|e| net_err(&e))?;
        let (mut checkout, _) = prepare
            .fetch_then_checkout(gix::progress::Discard, &interrupt)
            .map_err(|e| net_err(&e))?;
        checkout
            .main_worktree(gix::progress::Discard, &interrupt)
            .map_err(|e| net_err(&e))?;
        Ok(())
    }

    /// Remote tip of `refs/heads/<branch>` for local remotes; network remotes use a fetch.
    pub fn ls_remote(&self, remote: &str, branch: &str) -> Result<Option<Oid>> {
        check_ref_arg(remote)?;
        check_ref_arg(branch)?;
        if let Some(path) = self.local_remote_path(&self.repo()?, remote)? {
            let remote_repo = gix::open(path).map_err(|e| net_err(&e))?;
            let tip = remote_repo
                .try_find_reference(format!("refs/heads/{branch}").as_str())
                .map_err(|e| net_err(&e))?
                .and_then(|mut r| r.peel_to_id().ok().map(|id| from_gix(id.detach())));
            return Ok(tip);
        }
        Err(GitError::Unsupported(
            "ls-remote over the network requires system git".into(),
        ))
    }

    fn local_remote_path(&self, repo: &gix::Repository, remote: &str) -> Result<Option<PathBuf>> {
        let remote = repo.find_remote(remote).map_err(GitError::other)?;
        let Some(url) = remote.url(gix::remote::Direction::Push) else {
            return Err(GitError::other("remote has no url"));
        };
        if url.scheme == gix::url::Scheme::File {
            let raw = url.path.to_string();
            let path = PathBuf::from(raw);
            let path = if path.is_relative() {
                self.path.join(path)
            } else {
                path
            };
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    fn head_name(repo: &gix::Repository) -> String {
        repo.head_name().ok().flatten().map_or_else(
            || "refs/heads/main".to_string(),
            |n| n.as_bstr().to_string(),
        )
    }

    fn set_ref(
        repo: &gix::Repository,
        name: &str,
        new: gix::ObjectId,
        constraint: PreviousValue,
        log: &str,
    ) -> Result<()> {
        repo.reference(name, new, constraint, log).map_err(|e| {
            let msg = e.to_string();
            let mut full = msg.clone();
            let mut src = std::error::Error::source(&e);
            while let Some(s) = src {
                full.push_str(&s.to_string());
                src = s.source();
            }
            let low = full.to_ascii_lowercase();
            if low.contains("expected")
                || low.contains("exist")
                || low.contains("mismatch")
                || low.contains("lock")
            {
                GitError::RefChanged(name.to_string())
            } else {
                GitError::other(full)
            }
        })?;
        Ok(())
    }

    /// Copies every object reachable from `tip` that `dest` lacks.
    fn copy_objects(
        src: &gix::Repository,
        dest: &gix::Repository,
        tip: gix::ObjectId,
    ) -> Result<()> {
        let mut stack = vec![tip];
        while let Some(id) = stack.pop() {
            if dest.has_object(id) {
                continue;
            }
            let obj = src.find_object(id).map_err(GitError::other)?;
            let kind = obj.kind;
            let data = obj.data.clone();
            dest.write_buf(kind, &data).map_err(GitError::other)?;
            match kind {
                gix::object::Kind::Commit => {
                    let commit = obj.into_commit();
                    let decoded = commit.decode().map_err(GitError::other)?;
                    stack.push(decoded.tree());
                    stack.extend(decoded.parents());
                }
                gix::object::Kind::Tree => {
                    let tree = obj.into_tree();
                    for entry in tree.iter() {
                        let entry = entry.map_err(GitError::other)?;
                        if !entry.mode().is_commit() {
                            stack.push(entry.oid().to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn signature(repo: &gix::Repository) -> Result<gix::actor::Signature> {
        let sig = repo
            .committer()
            .ok_or_else(|| GitError::other("committer identity is not configured"))?
            .map_err(GitError::other)?;
        sig.to_owned().map_err(GitError::other)
    }

    fn index_tree(repo: &gix::Repository) -> Result<gix::ObjectId> {
        let index = repo.index_or_empty().map_err(GitError::other)?;
        let empty = repo.empty_tree().id;
        let mut editor = repo.edit_tree(empty).map_err(GitError::other)?;
        for entry in index.entries() {
            if entry.flags.stage_raw() != 0 {
                continue;
            }
            let kind = entry
                .mode
                .to_tree_entry_mode()
                .map_or(gix::object::tree::EntryKind::Blob, |m| m.kind());
            editor
                .upsert(entry.path(&index).to_string().as_str(), kind, entry.id)
                .map_err(GitError::other)?;
        }
        Ok(editor.write().map_err(GitError::other)?.detach())
    }
}

impl GitBackend for GixBackend {
    fn fetch(&self, remote_name: &str, branch: &str) -> Result<FetchOutcome> {
        check_ref_arg(remote_name)?;
        check_ref_arg(branch)?;
        let repo = self.repo()?;
        let tracking = format!("refs/remotes/{remote_name}/{branch}");
        let read_tracking = |repo: &gix::Repository| {
            repo.try_find_reference(tracking.as_str())
                .ok()
                .flatten()
                .and_then(|mut r| r.peel_to_id().ok().map(|id| from_gix(id.detach())))
        };
        let before = read_tracking(&repo);
        let spec = format!("+refs/heads/{branch}:{tracking}");
        let mut remote = repo.find_remote(remote_name).map_err(GitError::other)?;
        remote
            .replace_refspecs([spec.as_str()], gix::remote::Direction::Fetch)
            .map_err(GitError::other)?;
        let remote = remote.with_fetch_tags(gix::remote::fetch::Tags::None);
        let interrupt = AtomicBool::new(false);
        let connection = remote
            .connect(gix::remote::Direction::Fetch)
            .map_err(|e| net_err(&e))?;
        let prepared = connection
            .prepare_fetch(gix::progress::Discard, Default::default())
            .map_err(|e| net_err(&e))?;
        prepared
            .receive(gix::progress::Discard, &interrupt)
            .map_err(|e| net_err(&e))?;
        // Re-open so ref and object caches are fresh.
        let after = read_tracking(&self.repo()?);
        if after.is_none() {
            return Err(GitError::RemoteRefNotFound(format!(
                "{remote_name}/{branch}"
            )));
        }
        Ok(FetchOutcome {
            updated: before != after,
            remote_head: after,
        })
    }

    fn push(&self, remote_name: &str, branch: &str) -> Result<PushOutcome> {
        check_ref_arg(remote_name)?;
        check_ref_arg(branch)?;
        let repo = self.repo()?;
        let Some(remote_path) = self.local_remote_path(&repo, remote_name)? else {
            return Err(GitError::Unsupported(
                "pushing over the network without system git is not supported; install git".into(),
            ));
        };
        let head = repo
            .head_id()
            .map_err(|_| GitError::other("nothing to push: HEAD has no commits"))?
            .detach();
        let dest = gix::open(&remote_path).map_err(|e| net_err(&e))?;
        let ref_name = format!("refs/heads/{branch}");
        let remote_tip = dest
            .try_find_reference(ref_name.as_str())
            .map_err(GitError::other)?
            .and_then(|mut r| r.peel_to_id().ok().map(|id| id.detach()));
        if remote_tip == Some(head) {
            return Ok(PushOutcome { pushed: false });
        }
        if let Some(tip) = remote_tip {
            // Fast-forward only: the remote tip must be an ancestor of HEAD (and known locally).
            let known = repo.has_object(tip);
            let ff = known
                && repo
                    .merge_base(tip, head)
                    .map(|b| b.detach() == tip)
                    .unwrap_or(false);
            if !ff {
                return Err(GitError::NonFastForward);
            }
        }
        Self::copy_objects(&repo, &dest, head)?;
        let constraint = match remote_tip {
            Some(tip) => PreviousValue::MustExistAndMatch(Target::Object(tip)),
            None => PreviousValue::MustNotExist,
        };
        Self::set_ref(&dest, &ref_name, head, constraint, "push: fast-forward")?;
        let tracking = format!("refs/remotes/{remote_name}/{branch}");
        Self::set_ref(
            &repo,
            &tracking,
            head,
            PreviousValue::Any,
            "push: update tracking ref",
        )?;
        Ok(PushOutcome { pushed: true })
    }

    fn merge_base(&self, a: &Oid, b: &Oid) -> Result<Option<Oid>> {
        gix_read::merge_base(&self.repo()?, a, b)
    }

    fn read_blob(&self, commit: &Oid, path: &str) -> Result<Option<Vec<u8>>> {
        gix_read::read_blob(&self.repo()?, commit, path)
    }

    fn diff_trees(&self, a: &Oid, b: &Oid) -> Result<Vec<TreeChange>> {
        gix_trees::diff_trees(&self.repo()?, a, b)
    }

    fn write_tree(&self, base: &Oid, edits: &[TreeEdit]) -> Result<Oid> {
        gix_trees::write_tree(&self.repo()?, base, edits)
    }

    fn commit(&self, msg: &CommitMessage, opts: CommitOpts) -> Result<Oid> {
        let repo = self.repo()?;
        let head = repo.head_id().ok().map(|id| id.detach());
        let mut editor_base = Self::index_tree(&repo)?;
        if opts.stage_all {
            let status = gix_read::status(&repo)?;
            let workdir = repo
                .workdir()
                .ok_or_else(|| GitError::other("bare repository has no work tree"))?
                .to_path_buf();
            let mut editor = repo.edit_tree(editor_base).map_err(GitError::other)?;
            for d in &status.dirty {
                let full = workdir.join(&d.path);
                match d.kind {
                    DirtyKind::Deleted => {
                        editor.remove(d.path.as_str()).map_err(GitError::other)?;
                    }
                    DirtyKind::Untracked | DirtyKind::Modified => {
                        let meta = std::fs::symlink_metadata(&full)?;
                        let (kind, bytes) = if meta.file_type().is_symlink() {
                            let target = std::fs::read_link(&full)?;
                            (
                                gix::object::tree::EntryKind::Link,
                                target.to_string_lossy().into_owned().into_bytes(),
                            )
                        } else {
                            #[cfg(unix)]
                            let exec = {
                                use std::os::unix::fs::PermissionsExt;
                                meta.permissions().mode() & 0o111 != 0
                            };
                            #[cfg(not(unix))]
                            let exec = false;
                            (
                                if exec {
                                    gix::object::tree::EntryKind::BlobExecutable
                                } else {
                                    gix::object::tree::EntryKind::Blob
                                },
                                std::fs::read(&full)?,
                            )
                        };
                        let blob = repo.write_blob(bytes).map_err(GitError::other)?;
                        editor
                            .upsert(d.path.as_str(), kind, blob.detach())
                            .map_err(GitError::other)?;
                    }
                }
            }
            editor_base = editor.write().map_err(GitError::other)?.detach();
        }
        let tree = editor_base;

        let head_commit = head
            .map(|h| repo.find_commit(h))
            .transpose()
            .map_err(GitError::other)?;
        let parents: Vec<gix::ObjectId> = match (&head_commit, opts.amend) {
            (Some(c), true) => c.parent_ids().map(|p| p.detach()).collect(),
            (Some(c), false) => vec![c.id],
            (None, _) => Vec::new(),
        };
        if !opts.allow_empty && !opts.amend {
            let head_tree = head_commit
                .as_ref()
                .and_then(|c| c.tree_id().ok())
                .map_or_else(|| repo.empty_tree().id, |t| t.detach());
            if head_tree == tree {
                return Err(GitError::other("nothing to commit"));
            }
        }
        let sig = Self::signature(&repo)?;
        let commit = gix::objs::Commit {
            tree,
            parents: parents.into(),
            author: sig.clone(),
            committer: sig,
            encoding: None,
            message: msg.render().into(),
            extra_headers: Vec::new(),
        };
        let id = repo
            .write_object(&commit)
            .map_err(GitError::other)?
            .detach();
        let constraint = match head {
            Some(h) => PreviousValue::MustExistAndMatch(Target::Object(h)),
            None => PreviousValue::MustNotExist,
        };
        let branch_ref = Self::head_name(&repo);
        Self::set_ref(
            &repo,
            &branch_ref,
            id,
            constraint,
            &format!("commit: {}", msg.subject),
        )?;
        // Keep the index in sync with the new HEAD tree.
        let mut index = repo.index_from_tree(&tree).map_err(GitError::other)?;
        index.write(Default::default()).map_err(GitError::other)?;
        Ok(from_gix(id))
    }

    fn commit_tree(&self, tree: &Oid, parents: &[Oid], msg: &CommitMessage) -> Result<Oid> {
        let repo = self.repo()?;
        let sig = Self::signature(&repo)?;
        let parents: Result<Vec<gix::ObjectId>> = parents.iter().map(to_gix).collect();
        let commit = gix::objs::Commit {
            tree: to_gix(tree)?,
            parents: parents?.into(),
            author: sig.clone(),
            committer: sig,
            encoding: None,
            message: msg.render().into(),
            extra_headers: Vec::new(),
        };
        let id = repo.write_object(&commit).map_err(GitError::other)?;
        Ok(from_gix(id.detach()))
    }

    fn update_ref(&self, name: &str, new: &Oid, expected_old: Option<&Oid>) -> Result<()> {
        check_ref_arg(name)?;
        let repo = self.repo()?;
        let constraint = match expected_old {
            Some(old) => PreviousValue::MustExistAndMatch(Target::Object(to_gix(old)?)),
            None => PreviousValue::MustNotExist,
        };
        Self::set_ref(
            &repo,
            name,
            to_gix(new)?,
            constraint,
            "bitacora: update-ref",
        )
    }

    fn status(&self) -> Result<RepoStatus> {
        gix_read::status(&self.repo()?)
    }

    fn resolve_ref(&self, name: &str) -> Result<Option<Oid>> {
        check_ref_arg(name)?;
        gix_read::resolve_ref(&self.repo()?, name)
    }

    fn commit_info(&self, commit: &Oid) -> Result<CommitInfo> {
        gix_read::commit_info(&self.repo()?, commit)
    }

    fn reset_index(&self, commit: &Oid) -> Result<()> {
        let repo = self.repo()?;
        let tree = gix_read::tree_of(&repo, commit)?.id;
        let mut index = repo.index_from_tree(&tree).map_err(GitError::other)?;
        index.write(Default::default()).map_err(GitError::other)?;
        Ok(())
    }

    fn kind(&self) -> ActiveBackend {
        ActiveBackend::GixOnly
    }
}
