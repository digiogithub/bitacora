//! gix-based reads: blobs, merge-base and status. All gix API usage for reads lives in this
//! module and its siblings `gix_trees` and `gix_net` (ADR-007).

use std::collections::BTreeMap;

use super::{CommitInfo, DirtyKind, DirtyPath, GitError, Oid, RepoStatus, Result, UnmergedEntry};

pub(super) fn to_gix(oid: &Oid) -> Result<gix::ObjectId> {
    gix::ObjectId::from_hex(oid.as_hex().as_bytes()).map_err(GitError::other)
}

pub(super) fn from_gix(id: impl AsRef<gix::oid>) -> Oid {
    Oid(id.as_ref().to_hex().to_string())
}

pub(super) fn tree_of<'r>(repo: &'r gix::Repository, id: &Oid) -> Result<gix::Tree<'r>> {
    repo.find_object(to_gix(id)?)
        .map_err(GitError::other)?
        .peel_to_tree()
        .map_err(GitError::other)
}

pub(super) fn merge_base(repo: &gix::Repository, a: &Oid, b: &Oid) -> Result<Option<Oid>> {
    match repo.merge_base(to_gix(a)?, to_gix(b)?) {
        Ok(id) => Ok(Some(from_gix(id.detach()))),
        // gix reports "no merge base" as an error; objects missing from the odb are a real failure.
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("merge-base") {
                Ok(None)
            } else {
                Err(GitError::other(msg))
            }
        }
    }
}

pub(super) fn read_blob(
    repo: &gix::Repository,
    commit: &Oid,
    path: &str,
) -> Result<Option<Vec<u8>>> {
    let tree = tree_of(repo, commit)?;
    let Some(entry) = tree.lookup_entry_by_path(path).map_err(GitError::other)? else {
        return Ok(None);
    };
    if !entry.mode().is_blob() {
        return Ok(None);
    }
    let object = entry.object().map_err(GitError::other)?;
    Ok(Some(object.detach().data))
}

pub(super) fn resolve_ref(repo: &gix::Repository, name: &str) -> Result<Option<Oid>> {
    match repo.try_find_reference(name) {
        Ok(Some(mut r)) => match r.peel_to_id() {
            Ok(id) => Ok(Some(from_gix(id.detach()))),
            Err(_) => Ok(None),
        },
        Ok(None) => Ok(None),
        Err(e) => Err(GitError::other(e)),
    }
}

pub(super) fn commit_info(repo: &gix::Repository, id: &Oid) -> Result<CommitInfo> {
    let commit = repo.find_commit(to_gix(id)?).map_err(GitError::other)?;
    let decoded = commit.decode().map_err(GitError::other)?;
    let committer_time = decoded.committer().map_err(GitError::other)?.seconds();
    Ok(CommitInfo {
        id: id.clone(),
        tree: from_gix(commit.tree_id().map_err(GitError::other)?.detach()),
        parents: commit.parent_ids().map(|p| from_gix(p.detach())).collect(),
        committer_time,
        message: String::from_utf8_lossy(decoded.message).into_owned(),
    })
}

pub(super) fn status(repo: &gix::Repository) -> Result<RepoStatus> {
    let head = repo.head_id().ok().map(|id| from_gix(id.detach()));
    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .map(|n| n.shorten().to_string());

    // Unmerged entries straight from the index stages.
    let index = repo.index_or_empty().map_err(GitError::other)?;
    let mut unmerged: BTreeMap<String, UnmergedEntry> = BTreeMap::new();
    for entry in index.entries() {
        let stage = entry.flags.stage_raw();
        if stage == 0 {
            continue;
        }
        let path = entry.path(&index).to_string();
        let slot = unmerged.entry(path.clone()).or_insert(UnmergedEntry {
            path,
            base: None,
            ours: None,
            theirs: None,
        });
        let id = Some(from_gix(entry.id));
        match stage {
            1 => slot.base = id,
            2 => slot.ours = id,
            3 => slot.theirs = id,
            _ => {}
        }
    }

    let platform = repo
        .status(gix::progress::Discard)
        .map_err(GitError::other)?
        .untracked_files(gix::status::UntrackedFiles::Files);
    let iter = platform
        .into_index_worktree_iter(Vec::new())
        .map_err(GitError::other)?;
    let mut dirty = Vec::new();
    for item in iter {
        let item = item.map_err(GitError::other)?;
        use gix::status::index_worktree::iter::Summary as S;
        let Some(summary) = item.summary() else {
            continue;
        };
        let kind = match summary {
            S::Removed => DirtyKind::Deleted,
            S::Added => DirtyKind::Untracked,
            S::Modified | S::TypeChange => DirtyKind::Modified,
            S::Conflict | S::IntentToAdd | S::Renamed | S::Copied => continue,
        };
        dirty.push(DirtyPath {
            path: item.rela_path().to_string(),
            kind,
        });
    }
    dirty.sort_by(|a, b| a.path.cmp(&b.path));
    dirty.dedup();
    Ok(RepoStatus {
        head,
        branch,
        dirty,
        unmerged: unmerged.into_values().collect(),
    })
}
