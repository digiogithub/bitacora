//! External changes to loaded pages (BIT-US-0068, BIT-US-0069, BIT-US-0070; BIT-SP-0005.R13-R16;
//! `docs/design/block-editor.md` sections 4 and 6, ADR-016, ADR-017).
//!
//! The file of a loaded page changed on disk (git pull, Logseq, a sync tool):
//!
//! * the page has no unsaved edits: it is re-parsed and the new blocks are *aligned* with the old
//!   ones ([`align`], the block matcher of `bitacora-merge`), so surviving blocks keep their
//!   session [`BlockId`]s and undo entries, selection and scroll keep addressing them;
//! * the page has unsaved edits: `merge_page(base, ours, theirs)` runs with base = the bytes we
//!   last read or wrote (in memory only, ADR-017), ours = the in-memory page, theirs = the new
//!   bytes. A clean merge replaces the page content (ids stay stable, the page stays dirty and is
//!   flushed normally); a conflict keeps ours untouched, parks the page as *conflicted* (no
//!   writes) and records a [`ConflictNotice`] for the UI. Nothing is ever written with conflict
//!   markers;
//! * a page with no merge base (never written) and unsaved edits goes straight to the conflict
//!   notice, whose diff is a two-way per-block comparison.
//!
//! None of this is an undoable edit: the transaction history is not touched. The block being
//! edited in the UI ([`Workspace::set_editing_block`]) is reported through
//! [`ExternalEvent::EditingBlockChanged`] when the external change touched its text, so the app
//! can keep its buffer and offer keep mine / take disk / keep both on commit.

use std::sync::Arc;

use bitacora_merge::{MergeEnv, MergePage, Note, PageConflict, match_pages, merge_page};

use super::model::{BlockId, DiskSnapshot, Page};
use super::workspace::Workspace;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;

/// Aligns the blocks of `old` with those of `new` (both page bytes): uuid (`id::`) first, then
/// per matched parent an LCS over content, exact reorders and fuzzy matches. Returns, for every
/// block of `old` in document order, the index in `new` (document order) it became, if any.
/// Blocks of `new` that nobody maps to are additions; `None` entries are removals.
#[must_use]
pub fn align(old: &[u8], new: &[u8]) -> Vec<Option<usize>> {
    let (old, new) = (
        MergePage::parse(&String::from_utf8_lossy(old)),
        MergePage::parse(&String::from_utf8_lossy(new)),
    );
    match_pages(&old, &new)
}

/// How a block differs between our version and the disk version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// The block exists on both sides with different text.
    Changed,
    /// Only in our (unsaved) version.
    OnlyMine,
    /// Only in the disk version.
    OnlyDisk,
}

/// One entry of the per-block diff shown by "Show diff".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDiff {
    /// What differs.
    pub kind: DiffKind,
    /// Ancestor titles (outermost first) of the block.
    pub breadcrumb: Vec<String>,
    /// Our text (`None` for [`DiffKind::OnlyDisk`]).
    pub mine: Option<String>,
    /// The disk text (`None` for [`DiffKind::OnlyMine`]).
    pub disk: Option<String>,
}

/// Two-way per-block diff of two page versions (no merge base needed): matched blocks whose text
/// differs, then blocks only on one side. Identical blocks are omitted.
#[must_use]
pub fn diff_blocks(mine: &[u8], disk: &[u8]) -> Vec<BlockDiff> {
    let m = MergePage::parse(&String::from_utf8_lossy(mine));
    let d = MergePage::parse(&String::from_utf8_lossy(disk));
    let pairs = match_pages(&m, &d);
    let mut used = vec![false; d.blocks.len()];
    let mut out = Vec::new();
    for (i, p) in pairs.iter().enumerate() {
        match p {
            Some(j) => {
                used[*j] = true;
                let (a, b) = (m.raw_text(i).trim_end(), d.raw_text(*j).trim_end());
                if a != b {
                    out.push(BlockDiff {
                        kind: DiffKind::Changed,
                        breadcrumb: m.breadcrumb(i),
                        mine: Some(a.to_owned()),
                        disk: Some(b.to_owned()),
                    });
                }
            }
            None => out.push(BlockDiff {
                kind: DiffKind::OnlyMine,
                breadcrumb: m.breadcrumb(i),
                mine: Some(m.raw_text(i).trim_end().to_owned()),
                disk: None,
            }),
        }
    }
    for (j, u) in used.iter().enumerate() {
        if !u {
            out.push(BlockDiff {
                kind: DiffKind::OnlyDisk,
                breadcrumb: d.breadcrumb(j),
                mine: None,
                disk: Some(d.raw_text(j).trim_end().to_owned()),
            });
        }
    }
    out
}

/// Data of the "Page changed on disk" notice (BIT-US-0070): the page keeps our content, writes
/// are stopped until the user resolves it (`Request::Resolve`: keep mine / take disk).
#[derive(Debug, Clone)]
pub struct ConflictNotice {
    /// The conflicted page.
    pub key: PageKey,
    /// Its file.
    pub path: Option<GraphPath>,
    /// The version found on disk (what "take disk" loads).
    pub disk: DiskSnapshot,
    /// A merge base existed (the 3-way merge ran). `false` for a page we never wrote (ADR-017):
    /// `diff` is then the only information.
    pub base_available: bool,
    /// Field conflicts reported by the 3-way merge (empty without a base).
    pub conflicts: Vec<PageConflict>,
    /// Two-way per-block diff, our unsaved version against the disk version ("Show diff").
    pub diff: Vec<BlockDiff>,
}

/// What a reload changed in block terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReloadReport {
    /// Blocks that survived with their id.
    pub kept: Vec<BlockId>,
    /// Blocks that no longer exist: undo entries addressing them cannot be applied.
    pub removed: Vec<BlockId>,
    /// New blocks (fresh ids).
    pub added: Vec<BlockId>,
    /// Surviving blocks whose text changed.
    pub changed: Vec<BlockId>,
}

/// The block being edited in the UI changed outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditingConflict {
    /// Page.
    pub page: PageKey,
    /// The protected block (id is stable across the reload).
    pub block: BlockId,
    /// Its text before the external change (the UI buffer started from it).
    pub mine: String,
    /// Its text now, in the page model.
    pub disk: String,
}

/// Result of [`Workspace::apply_external`].
#[derive(Debug, Clone)]
pub enum ExternalOutcome {
    /// The bytes equal what we already have as the disk version.
    Unchanged,
    /// The page had no unsaved edits and was reloaded.
    Reloaded(ReloadReport),
    /// External edits were merged into the unsaved page; it stays dirty and is written next.
    Merged {
        /// Block-level effect.
        report: ReloadReport,
        /// Informational notes of the merge (competing moves...).
        notes: Vec<Note>,
    },
    /// Both sides changed the same thing: ours was kept, the page is conflicted.
    Conflict(Arc<ConflictNotice>),
    /// The page is not loaded.
    Unknown,
}

/// Something the writer thread should announce after an external change was applied (drained
/// with [`Workspace::take_external_events`]).
#[derive(Debug, Clone)]
pub enum ExternalEvent {
    /// A page was reloaded from disk.
    Reloaded(PageKey),
    /// External edits were merged into a dirty page.
    Merged(PageKey),
    /// A page is conflicted.
    Conflicted(Arc<ConflictNotice>),
    /// The block being edited changed on disk.
    EditingBlockChanged(EditingConflict),
}

impl Workspace {
    /// Sets (or clears) the block the UI is editing. External reloads report a change to it as
    /// [`ExternalEvent::EditingBlockChanged`].
    pub fn set_editing_block(&mut self, id: Option<BlockId>) {
        self.editing = id;
    }

    /// The notice of a conflicted page.
    #[must_use]
    pub fn conflict(&self, key: &PageKey) -> Option<&Arc<ConflictNotice>> {
        self.conflicted.get(key)
    }

    /// Drains the events produced by external changes applied since the last call.
    pub fn take_external_events(&mut self) -> Vec<ExternalEvent> {
        std::mem::take(&mut self.external_events)
    }

    /// Replaces the content of page `key` with `bytes`, keeping the ids of blocks aligned with
    /// the current ones. `disk` is the disk version recorded for the page and `dirty` whether it
    /// still has unsaved content relative to it. Not an undoable change.
    fn replace_aligned(
        &mut self,
        key: &PageKey,
        bytes: &[u8],
        disk: Option<DiskSnapshot>,
        dirty: bool,
    ) -> Option<ReloadReport> {
        let old = self.page(key)?;
        let old_order = old.dfs();
        let old_bytes = old.serialize();
        let old_page = MergePage::parse(&String::from_utf8_lossy(&old_bytes));
        let new_page = MergePage::parse(&String::from_utf8_lossy(bytes));
        let mut reuse: Vec<Option<BlockId>> = vec![None; new_page.blocks.len()];
        if old_page.blocks.len() == old_order.len() {
            for (i, m) in match_pages(&old_page, &new_page).iter().enumerate() {
                if let Some(j) = m {
                    reuse[*j] = Some(old_order[i]);
                }
            }
        }
        let mut fresh = Page::load_reusing(
            key.clone(),
            old.title.clone(),
            old.path.clone(),
            bytes,
            &self.ids,
            &reuse,
        );
        if fresh.dfs().len() != new_page.blocks.len() {
            // The two parsers disagree on the block count: do not trust the alignment.
            fresh = Page::load(
                key.clone(),
                old.title.clone(),
                old.path.clone(),
                bytes,
                &self.ids,
            );
        }
        fresh.disk_path.clone_from(&old.disk_path);
        fresh.read_only = old.read_only;
        fresh.lazy = old.lazy.clone();
        if let Some(d) = disk {
            fresh.disk = Some(d);
        }
        fresh.dirty = dirty;

        let old_texts: Vec<(BlockId, String)> = old_order
            .iter()
            .filter_map(|id| old.block(*id).map(|b| (*id, b.text.clone())))
            .collect();
        let mut report = ReloadReport::default();
        for (id, text) in &old_texts {
            match fresh.block(*id) {
                Some(nb) => {
                    report.kept.push(*id);
                    if nb.text != *text {
                        report.changed.push(*id);
                    }
                }
                None => report.removed.push(*id),
            }
        }
        report.added = fresh
            .dfs()
            .into_iter()
            .filter(|id| !old_texts.iter().any(|(o, _)| o == id))
            .collect();

        if let Some(ed) = self.editing
            && report.changed.contains(&ed)
        {
            let mine = old_texts
                .iter()
                .find(|(i, _)| *i == ed)
                .map(|(_, t)| t.clone())
                .unwrap_or_default();
            let disk = fresh.block(ed).map(|b| b.text.clone()).unwrap_or_default();
            self.external_events
                .push(ExternalEvent::EditingBlockChanged(EditingConflict {
                    page: key.clone(),
                    block: ed,
                    mine,
                    disk,
                }));
        }
        self.insert_page(fresh);
        Some(report)
    }

    /// Applies `bytes`, the new content of the file behind page `key`, as described in the module
    /// documentation. Never writes to disk.
    pub fn apply_external(&mut self, key: &PageKey, bytes: &[u8]) -> ExternalOutcome {
        let Some(page) = self.page(key) else {
            return ExternalOutcome::Unknown;
        };
        if page
            .disk
            .as_ref()
            .is_some_and(|d| d.hash == blake3::hash(bytes))
            && !self.conflicted.contains_key(key)
        {
            return ExternalOutcome::Unchanged;
        }
        // The file already holds exactly what we would write: nothing to reload or merge, and
        // block ids stay as they are (an identical echo must never change a page's identity).
        if !self.conflicted.contains_key(key) && page.serialize() == bytes {
            return ExternalOutcome::Unchanged;
        }
        if !page.needs_write() {
            return match self.replace_aligned(key, bytes, None, false) {
                Some(r) => {
                    self.external_events
                        .push(ExternalEvent::Reloaded(key.clone()));
                    ExternalOutcome::Reloaded(r)
                }
                None => ExternalOutcome::Unknown,
            };
        }
        let ours = page.serialize();
        let base = page.disk.as_ref().map(|d| Arc::clone(&d.bytes));
        let path = page.disk_path.clone().or_else(|| page.path.clone());
        let theirs_snapshot = DiskSnapshot::new(bytes);
        let merged = base.as_ref().and_then(|b| {
            let (b, o, t) = (
                std::str::from_utf8(b).ok()?,
                std::str::from_utf8(&ours).ok()?,
                std::str::from_utf8(bytes).ok()?,
            );
            Some(merge_page(b, o, t, &MergeEnv::new()))
        });
        match merged {
            Some(r) if r.conflicts.is_empty() => {
                let out = r.output.into_bytes();
                let still_dirty = out != bytes;
                match self.replace_aligned(key, &out, Some(theirs_snapshot), still_dirty) {
                    Some(report) => {
                        self.external_events
                            .push(ExternalEvent::Merged(key.clone()));
                        ExternalOutcome::Merged {
                            report,
                            notes: r.notes,
                        }
                    }
                    None => ExternalOutcome::Unknown,
                }
            }
            other => {
                let notice = Arc::new(ConflictNotice {
                    key: key.clone(),
                    path,
                    disk: theirs_snapshot,
                    base_available: other.is_some(),
                    conflicts: other.map(|r| r.conflicts).unwrap_or_default(),
                    diff: diff_blocks(&ours, bytes),
                });
                self.conflicted.insert(key.clone(), Arc::clone(&notice));
                self.external_events
                    .push(ExternalEvent::Conflicted(Arc::clone(&notice)));
                ExternalOutcome::Conflict(notice)
            }
        }
    }

    /// Reloads `key` from `bytes` keeping block ids stable (used by "take disk").
    pub(crate) fn reload_keeping_ids(&mut self, key: &PageKey, bytes: &[u8]) -> bool {
        self.replace_aligned(key, bytes, None, false).is_some()
    }
}
