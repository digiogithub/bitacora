//! Per-page history (BIT-US-0048, BIT-SP-0006.R22): the commits that touched one page, following
//! renames, with device and kind from the trailers, and block-level diffs between a historical
//! version and the current text.
//!
//! Everything goes through [`GitBackend`] reads (`commit_info`, `diff_trees`, `read_blob`), so it
//! works with every backend. The walk follows first parents only: a merge commit is listed when
//! it changed the page relative to its first parent (our side), which is what "what happened to
//! my page" means.

use bitacora_merge::diff_pages;
pub use bitacora_merge::{BlockDiff, DiffKind, MergePage, PageDiff};

use crate::backend::{CommitKind, GitBackend, GitError, Oid, TreeChange};
use crate::commit_msg::parse_message;

/// Commits inspected at most per query, so a huge repository cannot stall the UI.
pub const MAX_WALK: usize = 5000;

/// How a commit changed the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageChange {
    /// The page file was created.
    Added,
    /// The content changed.
    Modified,
    /// The file moved here from `from` (and possibly changed).
    Renamed {
        /// Previous path.
        from: String,
    },
    /// The file was deleted (and exists again in a later commit).
    Deleted,
}

/// One commit in the history of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The commit.
    pub commit: Oid,
    /// Commit whose tree holds the version this entry stands for (`commit`, or its parent for a
    /// deletion).
    pub content_commit: Oid,
    /// Path of the page at `content_commit`.
    pub path: String,
    /// Committer time, seconds since the epoch.
    pub time: i64,
    /// Subject line.
    pub subject: String,
    /// `Bitacora-Device` trailer.
    pub device: Option<String>,
    /// `Bitacora-Kind` trailer.
    pub kind: Option<CommitKind>,
    /// `Bitacora-Agent` trailer.
    pub agent: Option<String>,
    /// What the commit did to the page.
    pub change: PageChange,
}

/// Lists the commits (newest first) reachable from `tip` through first parents that touched
/// `path`, following renames backwards; at most `limit` entries.
///
/// # Errors
/// Any [`GitError`] from the backend.
pub fn page_history(
    backend: &dyn GitBackend,
    tip: &Oid,
    path: &str,
    limit: usize,
) -> Result<Vec<HistoryEntry>, GitError> {
    let mut out = Vec::new();
    let mut cur_path = path.to_owned();
    let mut cur = tip.clone();
    for _ in 0..MAX_WALK {
        if out.len() >= limit {
            break;
        }
        let info = backend.commit_info(&cur)?;
        let Some(parent) = info.parents.first().cloned() else {
            // Root commit: the page was created here if it exists.
            if backend.read_blob(&cur, &cur_path)?.is_some() {
                out.push(entry(&info, &cur, &cur_path, PageChange::Added));
            }
            break;
        };
        let mut next_path = None;
        let mut found = None;
        for change in backend.diff_trees(&parent, &cur)? {
            match change {
                TreeChange::Modified { path } if path == cur_path => {
                    found = Some((PageChange::Modified, cur.clone(), cur_path.clone()));
                }
                TreeChange::Added { path } if path == cur_path => {
                    found = Some((PageChange::Added, cur.clone(), cur_path.clone()));
                }
                TreeChange::Renamed { from, to, .. } if to == cur_path => {
                    found = Some((
                        PageChange::Renamed { from: from.clone() },
                        cur.clone(),
                        cur_path.clone(),
                    ));
                    next_path = Some(from);
                }
                TreeChange::Deleted { path } if path == cur_path => {
                    found = Some((PageChange::Deleted, parent.clone(), cur_path.clone()));
                }
                _ => {}
            }
        }
        if let Some((change, content_commit, p)) = found {
            let added = change == PageChange::Added;
            out.push(entry(&info, &content_commit, &p, change));
            if added {
                // Created here; an earlier incarnation (delete then re-create) is a different file
                // life, so the page's history ends.
                break;
            }
        }
        if let Some(p) = next_path {
            cur_path = p;
        }
        cur = parent;
    }
    Ok(out)
}

fn entry(
    info: &crate::backend::CommitInfo,
    content: &Oid,
    path: &str,
    change: PageChange,
) -> HistoryEntry {
    let parsed = parse_message(&info.message);
    HistoryEntry {
        commit: info.id.clone(),
        content_commit: content.clone(),
        path: path.to_owned(),
        time: info.committer_time,
        subject: parsed.subject,
        device: parsed.device,
        kind: parsed.kind,
        agent: parsed.agent,
        change,
    }
}

/// The text of the page at a history entry.
///
/// # Errors
/// Any [`GitError`]; a missing blob or non-UTF-8 content yields `None`.
pub fn version_text(
    backend: &dyn GitBackend,
    entry: &HistoryEntry,
) -> Result<Option<String>, GitError> {
    Ok(backend
        .read_blob(&entry.content_commit, &entry.path)?
        .and_then(|b| String::from_utf8(b).ok()))
}

/// Block-level diff between a historical `version` and the `current` text of the page
/// (`old` = version, `new` = current).
#[must_use]
pub fn diff_version(version: &str, current: &str) -> PageDiff {
    diff_pages(&MergePage::parse(version), &MergePage::parse(current))
}
