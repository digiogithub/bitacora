//! Invertible primitive operations (`docs/design/block-editor.md` §3.1).
//!
//! Every edit of the graph is a sequence of these. [`Op::apply`] fills the data an inverse needs
//! (captured subtrees, former positions) and [`Op::inverse`] builds the op that undoes it. Block
//! slots use [`Position`]: `index` counts the siblings *without* the block being placed, so a
//! move and its inverse are symmetric.

use std::ops::Range;

use super::model::{BlockId, Page, Position, Subtree};
use super::workspace::Workspace;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;

/// Why an op could not be applied or inverted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OpError {
    /// No loaded page with this key.
    #[error("unknown page {0:?}")]
    UnknownPage(PageKey),
    /// No block with this id.
    #[error("unknown block {0:?}")]
    UnknownBlock(BlockId),
    /// The page is read-only (`.org`).
    #[error("page {0:?} is read-only")]
    ReadOnly(PageKey),
    /// Insert index past the end of the sibling list.
    #[error("index {index} out of range (len {len})")]
    BadIndex {
        /// Requested index.
        index: usize,
        /// Sibling count.
        len: usize,
    },
    /// A block would become its own descendant.
    #[error("cannot move {0:?} into its own subtree")]
    MoveIntoDescendant(BlockId),
    /// A block id is already in use.
    #[error("block {0:?} already exists")]
    DuplicateBlock(BlockId),
    /// The op's expectation about the current state does not hold.
    #[error("stale op: {0}")]
    Stale(&'static str),
    /// The page already exists.
    #[error("page {0:?} already exists")]
    PageExists(PageKey),
    /// The target file is used by another page.
    #[error("path {0} is used by another page")]
    PathTaken(String),
    /// `inverse` was asked of an op that was not applied yet.
    #[error("op has not been applied, so it has no inverse")]
    NotApplied,
    /// The text contains a line that would re-parse as a new block (e.g. `x\n- y`).
    #[error("text cannot be stored in one block: a line starts like a list item")]
    Unrepresentable,
    /// Malformed op.
    #[error("invalid op: {0}")]
    Invalid(&'static str),
}

/// A primitive, invertible graph operation.
#[derive(Debug, Clone)]
pub enum Op {
    /// Inserts a detached subtree as child `index` of `parent` (`None` = page root).
    InsertSubtree {
        /// Page.
        page: PageKey,
        /// Parent block.
        parent: Option<BlockId>,
        /// Index among the siblings.
        index: usize,
        /// The subtree (carries its ids).
        subtree: Subtree,
    },
    /// Removes a block with its descendants; the subtree is captured by `apply`.
    RemoveSubtree {
        /// Page.
        page: PageKey,
        /// Root of the removed subtree.
        id: BlockId,
        /// Former slot and content, filled by `apply`.
        captured: Option<(Position, Subtree)>,
    },
    /// Moves a block with its subtree within a page or across pages.
    Move {
        /// The moved block.
        id: BlockId,
        /// Former slot, filled by `apply`.
        from: Option<Position>,
        /// Target slot (`index` counts siblings without the moved block).
        to: Position,
    },
    /// Replaces the whole text of a block.
    SetText {
        /// Block.
        id: BlockId,
        /// Expected current text.
        before: String,
        /// New text.
        after: String,
    },
    /// Replaces a byte range of a block's text (typing, big blocks).
    EditText {
        /// Block.
        id: BlockId,
        /// Byte range (on char boundaries) in the current text.
        range: Range<usize>,
        /// Expected current content of `range`.
        removed: String,
        /// Replacement.
        inserted: String,
    },
    /// Re-parents children of `from` under `to` at `at`.
    AdoptChildren {
        /// Block losing children.
        from: BlockId,
        /// Block gaining them.
        to: BlockId,
        /// Insert index in `to`'s children.
        at: usize,
        /// The moved children. Empty when planning = all children of `from`; `apply` fills it.
        moved: Vec<BlockId>,
        /// Index of the first moved child in `from`, filled by `apply`.
        src_at: Option<usize>,
    },
    /// Replaces the pre-block (front matter / page properties).
    SetPreamble {
        /// Page.
        page: PageKey,
        /// Expected current preamble.
        before: Option<String>,
        /// New preamble.
        after: Option<String>,
    },
    /// Creates an empty page, or reinstalls a deleted one (`restored`).
    CreatePage {
        /// Key of the new page.
        page: PageKey,
        /// Title.
        title: String,
        /// File the page will live in.
        path: Option<GraphPath>,
        /// A previously deleted page to bring back unchanged.
        restored: Option<Box<Page>>,
    },
    /// Deletes a page; the page is captured by `apply`.
    DeletePage {
        /// Page.
        page: PageKey,
        /// The removed page, filled by `apply`.
        captured: Option<Box<Page>>,
    },
    /// Moves a non-page file (an asset) to `logseq/.recycle/` at the next flush.
    DeleteAsset {
        /// Graph-relative path of the file.
        path: GraphPath,
    },
    /// Creates a new attachment (a pasted or dropped file) at the next flush, atomically. The
    /// file never replaces an existing one (the name carries a timestamp and an index).
    ImportAsset {
        /// Graph-relative path (`assets/x.png`).
        path: GraphPath,
        /// File content.
        bytes: std::sync::Arc<[u8]>,
    },
    /// Undo of [`Op::ImportAsset`]: cancels the pending create, or recycles the written file.
    DropAsset {
        /// Graph-relative path.
        path: GraphPath,
        /// The content, to import it again on redo.
        bytes: std::sync::Arc<[u8]>,
    },
    /// Undo of [`Op::DeleteAsset`]: cancels the pending recycle, or moves the recycled copy back.
    RestoreAsset {
        /// Graph-relative path of the file.
        path: GraphPath,
    },
    /// Gives a page a new key and title (the file is renamed by [`Op::RenameFile`]). With equal
    /// keys only the title changes (case-only rename).
    RenamePage {
        /// Current key.
        from: PageKey,
        /// New key.
        to: PageKey,
        /// Expected current title.
        title_from: String,
        /// New title.
        title_to: String,
    },
    /// Edits a non-page file (`logseq/config.edn`) from `before` to `after`. Written at the next
    /// flush, only when the file still holds the content it had when first edited.
    EditFile {
        /// Graph-relative path.
        path: GraphPath,
        /// Expected current content.
        before: Vec<u8>,
        /// New content.
        after: Vec<u8>,
    },
    /// Renames the file of a page (the title is unchanged).
    RenameFile {
        /// Page.
        page: PageKey,
        /// Expected current path.
        from: Option<GraphPath>,
        /// New path.
        to: Option<GraphPath>,
    },
}

fn stale<T>(why: &'static str) -> Result<T, OpError> {
    Err(OpError::Stale(why))
}

fn subtree_representable(st: &super::model::Subtree) -> bool {
    let loaded = st
        .origin
        .as_ref()
        .is_some_and(|o| o.text_hash == super::model::text_hash(&st.text));
    (loaded || super::model::text_is_representable(&st.text))
        && st.children.iter().all(subtree_representable)
}

impl Op {
    /// Applies the op, filling captured data. On error the graph is unchanged.
    ///
    /// # Errors
    /// [`OpError`] when the op does not fit the current state.
    pub fn apply(&mut self, ws: &mut Workspace) -> Result<(), OpError> {
        match self {
            Self::InsertSubtree {
                page,
                parent,
                index,
                subtree,
            } => {
                if !subtree_representable(subtree) {
                    return Err(OpError::Unrepresentable);
                }
                ws.attach(page, *parent, *index, subtree.clone())
            }
            Self::RemoveSubtree { page, id, captured } => {
                if ws.locate(*id) != Some(page) {
                    return Err(OpError::UnknownBlock(*id));
                }
                *captured = Some(ws.detach(*id)?);
                Ok(())
            }
            Self::Move { id, from, to } => {
                let cur = ws.position_of(*id).ok_or(OpError::UnknownBlock(*id))?;
                if let Some(p) = to.parent
                    && ws.is_within(*id, p)
                {
                    return Err(OpError::MoveIntoDescendant(*id));
                }
                let (pos, st) = ws.detach(*id)?;
                if let Err(e) = ws.attach(&to.page, to.parent, to.index, st.clone()) {
                    // Put it back where it was; the slot existed a moment ago.
                    ws.attach(&pos.page, pos.parent, pos.index, st)?;
                    return Err(e);
                }
                *from = Some(cur);
                Ok(())
            }
            Self::SetText { id, before, after } => {
                if ws.block(*id).ok_or(OpError::UnknownBlock(*id))?.text != *before {
                    return stale("text differs from `before`");
                }
                ws.set_text_raw(*id, after.clone())
            }
            Self::EditText {
                id,
                range,
                removed,
                inserted,
            } => {
                let text = &ws.block(*id).ok_or(OpError::UnknownBlock(*id))?.text;
                if text.get(range.clone()) != Some(removed.as_str()) {
                    return stale("range content differs from `removed`");
                }
                let mut new = String::with_capacity(text.len() + inserted.len());
                new.push_str(&text[..range.start]);
                new.push_str(inserted);
                new.push_str(&text[range.end..]);
                ws.set_text_raw(*id, new)
            }
            Self::AdoptChildren {
                from,
                to,
                at,
                moved,
                src_at,
            } => {
                let page = ws
                    .locate(*from)
                    .cloned()
                    .ok_or(OpError::UnknownBlock(*from))?;
                if ws.locate(*to) != Some(&page) {
                    return Err(OpError::Invalid("adopt across pages"));
                }
                if from == to {
                    return Err(OpError::Invalid("adopt into itself"));
                }
                let ids = if moved.is_empty() {
                    ws.block(*from)
                        .ok_or(OpError::UnknownBlock(*from))?
                        .children
                        .clone()
                } else {
                    moved.clone()
                };
                if ids.iter().any(|c| ws.is_within(*c, *to)) {
                    return Err(OpError::MoveIntoDescendant(*to));
                }
                let first = ws.reparent(&page, *from, *to, *at, &ids)?;
                *moved = ids;
                *src_at = Some(first);
                Ok(())
            }
            Self::SetPreamble {
                page,
                before,
                after,
            } => {
                let p = ws.writable_page(page)?;
                if p.preamble != *before {
                    return stale("preamble differs from `before`");
                }
                p.preamble.clone_from(after);
                ws.mark_touched(page);
                Ok(())
            }
            Self::CreatePage {
                page,
                title,
                path,
                restored,
            } => {
                if ws.page(page).is_some() {
                    return Err(OpError::PageExists(page.clone()));
                }
                if let Some(path) = path
                    && ws.page_for_path(path).is_some()
                {
                    return Err(OpError::PathTaken(path.to_string()));
                }
                let mut p = match restored {
                    Some(p) => (**p).clone(),
                    None => Page::empty(page.clone(), title.clone(), path.clone()),
                };
                // A restored page may have lost its file meanwhile: write it again.
                p.dirty = true;
                if let Some(dp) = &p.disk_path {
                    ws.unqueue_delete(dp);
                }
                ws.insert_page(p);
                ws.mark_touched(page);
                Ok(())
            }
            Self::DeletePage { page, captured } => {
                ws.writable_page(page)?;
                let p = ws
                    .remove_page(page)
                    .ok_or_else(|| OpError::UnknownPage(page.clone()))?;
                if let Some(dp) = &p.disk_path {
                    ws.queue_delete(dp.clone(), p.disk.as_ref().map(|d| d.bytes.clone()));
                }
                ws.touched.insert(page.clone());
                *captured = Some(Box::new(p));
                Ok(())
            }
            Self::DeleteAsset { path } => {
                if ws.page_for_path(path).is_some() {
                    return Err(OpError::Invalid("a page file is deleted with DeletePage"));
                }
                ws.unqueue_restore(path);
                ws.queue_delete(path.clone(), None);
                Ok(())
            }
            Self::ImportAsset { path, bytes } => {
                if ws.page_for_path(path).is_some() {
                    return Err(OpError::Invalid("an attachment cannot replace a page file"));
                }
                ws.unqueue_delete(path);
                ws.queue_create(path.clone(), bytes.clone());
                Ok(())
            }
            Self::DropAsset { path, .. } => {
                if !ws.unqueue_create(path) {
                    ws.queue_delete(path.clone(), None);
                }
                Ok(())
            }
            Self::RestoreAsset { path } => {
                if ws.pending_deletes().contains_key(path) {
                    ws.unqueue_delete(path);
                } else {
                    ws.queue_restore(path.clone());
                }
                Ok(())
            }
            Self::RenamePage {
                from,
                to,
                title_from,
                title_to,
            } => {
                if ws.writable_page(from)?.title != *title_from {
                    return stale("title differs from `title_from`");
                }
                ws.rekey_page(from, to, title_to)
            }
            Self::EditFile {
                path,
                before,
                after,
            } => {
                if ws.page_for_path(path).is_some() {
                    return Err(OpError::Invalid("a page file is edited through its page"));
                }
                ws.queue_edit(path, before, after)
            }
            Self::RenameFile { page, from, to } => {
                if ws.writable_page(page)?.path != *from {
                    return stale("path differs from `from`");
                }
                if let Some(t) = to
                    && ws.page_for_path(t).is_some_and(|k| k != page)
                {
                    return Err(OpError::PathTaken(t.to_string()));
                }
                ws.writable_page(page)?.path.clone_from(to);
                ws.mark_touched(page);
                Ok(())
            }
        }
    }

    /// The op that undoes this one. Requires a prior [`Op::apply`] for ops that capture data.
    ///
    /// # Errors
    /// [`OpError::NotApplied`] when captured data is missing.
    pub fn inverse(&self) -> Result<Op, OpError> {
        Ok(match self {
            Self::InsertSubtree { page, subtree, .. } => Self::RemoveSubtree {
                page: page.clone(),
                id: subtree.id,
                captured: None,
            },
            Self::RemoveSubtree { captured, .. } => {
                let (pos, st) = captured.as_ref().ok_or(OpError::NotApplied)?;
                Self::InsertSubtree {
                    page: pos.page.clone(),
                    parent: pos.parent,
                    index: pos.index,
                    subtree: st.clone(),
                }
            }
            Self::Move { id, from, to } => Self::Move {
                id: *id,
                from: Some(to.clone()),
                to: from.clone().ok_or(OpError::NotApplied)?,
            },
            Self::SetText { id, before, after } => Self::SetText {
                id: *id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::EditText {
                id,
                range,
                removed,
                inserted,
            } => Self::EditText {
                id: *id,
                range: range.start..range.start + inserted.len(),
                removed: inserted.clone(),
                inserted: removed.clone(),
            },
            Self::AdoptChildren {
                from,
                to,
                at,
                moved,
                src_at,
            } => Self::AdoptChildren {
                from: *to,
                to: *from,
                at: src_at.ok_or(OpError::NotApplied)?,
                moved: moved.clone(),
                src_at: Some(*at),
            },
            Self::SetPreamble {
                page,
                before,
                after,
            } => Self::SetPreamble {
                page: page.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            Self::CreatePage { page, .. } => Self::DeletePage {
                page: page.clone(),
                captured: None,
            },
            Self::DeletePage { page, captured } => {
                let p = captured.as_ref().ok_or(OpError::NotApplied)?;
                Self::CreatePage {
                    page: page.clone(),
                    title: p.title.clone(),
                    path: p.path.clone(),
                    restored: Some(p.clone()),
                }
            }
            Self::ImportAsset { path, bytes } => Self::DropAsset {
                path: path.clone(),
                bytes: bytes.clone(),
            },
            Self::DropAsset { path, bytes } => Self::ImportAsset {
                path: path.clone(),
                bytes: bytes.clone(),
            },
            Self::DeleteAsset { path } => Self::RestoreAsset { path: path.clone() },
            Self::RestoreAsset { path } => Self::DeleteAsset { path: path.clone() },
            Self::RenamePage {
                from,
                to,
                title_from,
                title_to,
            } => Self::RenamePage {
                from: to.clone(),
                to: from.clone(),
                title_from: title_to.clone(),
                title_to: title_from.clone(),
            },
            Self::EditFile {
                path,
                before,
                after,
            } => Self::EditFile {
                path: path.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            Self::RenameFile { page, from, to } => Self::RenameFile {
                page: page.clone(),
                from: to.clone(),
                to: from.clone(),
            },
        })
    }
}
