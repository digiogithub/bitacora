//! gix tree diff with rename detection and tree writing (no work-tree or index mutation).

use std::collections::{BTreeMap, BTreeSet};

use gix::bstr::ByteSlice;
use gix::object::tree::EntryKind;

use super::gix_read::{from_gix, tree_of};
use super::{GitError, Oid, Result, TreeChange, TreeEdit};

/// Minimum similarity (fraction) for ordinary rename detection (design: >= 50%).
const RENAME_SIMILARITY: f32 = 0.5;

pub(super) fn diff_trees(repo: &gix::Repository, a: &Oid, b: &Oid) -> Result<Vec<TreeChange>> {
    let old = tree_of(repo, a)?;
    let new = tree_of(repo, b)?;
    let options = gix::diff::Options::default().with_rewrites(Some(gix::diff::Rewrites {
        copies: None,
        percentage: Some(RENAME_SIMILARITY),
        limit: 1000,
        track_empty: false,
    }));
    let raw = repo
        .diff_tree_to_tree(&old, &new, options)
        .map_err(GitError::other)?;

    use gix::object::tree::diff::ChangeDetached as C;
    let mut changes = Vec::new();
    let mut added: BTreeMap<String, gix::ObjectId> = BTreeMap::new();
    let mut deleted: BTreeMap<String, gix::ObjectId> = BTreeMap::new();
    for change in raw {
        match change {
            C::Addition {
                location,
                entry_mode,
                id,
                ..
            } => {
                if entry_mode.is_blob() {
                    added.insert(location.to_str_lossy().into_owned(), id);
                }
            }
            C::Deletion {
                location,
                entry_mode,
                id,
                ..
            } => {
                if entry_mode.is_blob() {
                    deleted.insert(location.to_str_lossy().into_owned(), id);
                }
            }
            C::Modification {
                location,
                entry_mode,
                ..
            } => {
                if entry_mode.is_blob() {
                    changes.push(TreeChange::Modified {
                        path: location.to_str_lossy().into_owned(),
                    });
                }
            }
            C::Rewrite {
                source_location,
                location,
                source_id,
                id,
                diff,
                ..
            } => {
                let similarity = match diff {
                    None => 100,
                    Some(stats) => {
                        let denom = stats.before.max(stats.after).max(1) as f32;
                        let kept = (stats.before as f32 - stats.removals as f32).max(0.0);
                        ((kept / denom) * 100.0).round().clamp(0.0, 100.0) as u8
                    }
                };
                let _ = (source_id, id);
                changes.push(TreeChange::Renamed {
                    from: source_location.to_str_lossy().into_owned(),
                    to: location.to_str_lossy().into_owned(),
                    similarity,
                });
            }
        }
    }

    // Strong-signal pass: Deleted+Added `.md` pairs whose non-empty `id::` sets are identical are
    // renames even below the similarity threshold.
    let mut paired_added: BTreeSet<String> = BTreeSet::new();
    let mut paired_deleted: BTreeSet<String> = BTreeSet::new();
    let mut deleted_ids: Vec<(String, BTreeSet<String>)> = Vec::new();
    for (path, id) in &deleted {
        if path.ends_with(".md") {
            let set = block_id_set(repo, *id);
            if !set.is_empty() {
                deleted_ids.push((path.clone(), set));
            }
        }
    }
    for (apath, aid) in &added {
        if !apath.ends_with(".md") {
            continue;
        }
        let aset = block_id_set(repo, *aid);
        if aset.is_empty() {
            continue;
        }
        if let Some((dpath, _)) = deleted_ids
            .iter()
            .find(|(dpath, dset)| !paired_deleted.contains(dpath) && *dset == aset)
        {
            paired_deleted.insert(dpath.clone());
            paired_added.insert(apath.clone());
            changes.push(TreeChange::Renamed {
                from: dpath.clone(),
                to: apath.clone(),
                similarity: 0,
            });
        }
    }
    for path in added.keys().filter(|p| !paired_added.contains(*p)) {
        changes.push(TreeChange::Added { path: path.clone() });
    }
    for path in deleted.keys().filter(|p| !paired_deleted.contains(*p)) {
        changes.push(TreeChange::Deleted { path: path.clone() });
    }
    changes.sort_by(|x, y| sort_key(x).cmp(sort_key(y)));
    Ok(changes)
}

fn sort_key(c: &TreeChange) -> &str {
    match c {
        TreeChange::Added { path }
        | TreeChange::Deleted { path }
        | TreeChange::Modified { path } => path,
        TreeChange::Renamed { to, .. } => to,
    }
}

/// The set of `id::` property values appearing in a Markdown blob.
fn block_id_set(repo: &gix::Repository, id: gix::ObjectId) -> BTreeSet<String> {
    let Ok(obj) = repo.find_object(id) else {
        return BTreeSet::new();
    };
    let text = String::from_utf8_lossy(&obj.data);
    text.lines()
        .filter_map(|line| {
            let t = line.trim_start().trim_start_matches("- ").trim_start();
            t.strip_prefix("id::")
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .collect()
}

pub(super) fn write_tree(repo: &gix::Repository, base: &Oid, edits: &[TreeEdit]) -> Result<Oid> {
    let base_tree = tree_of(repo, base)?;
    let mut editor = repo.edit_tree(base_tree.id).map_err(GitError::other)?;
    for edit in edits {
        match edit {
            TreeEdit::Upsert { path, content } => {
                let blob = repo.write_blob(content).map_err(GitError::other)?;
                editor
                    .upsert(path.as_str(), EntryKind::Blob, blob.detach())
                    .map_err(GitError::other)?;
            }
            TreeEdit::Remove { path } => {
                editor.remove(path.as_str()).map_err(GitError::other)?;
            }
        }
    }
    let id = editor.write().map_err(GitError::other)?;
    Ok(from_gix(id.detach()))
}
