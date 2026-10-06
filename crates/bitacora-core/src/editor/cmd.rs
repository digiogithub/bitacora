//! Commands: user intents compiled into ops by pure planners
//! (`docs/design/block-editor.md` §3.2). This is the skeleton; further commands are added by the
//! editing stories.

use std::ops::Range;

use bitacora_markdown::edit::properties::{set_front_matter_property, set_property};
use bitacora_markdown::edit::state::{CollapseMode, set_collapsed};

use super::model::{BlockId, Position, Subtree};
use super::op::Op;
use super::workspace::Workspace;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;

/// Where [`Cmd::MoveBlocks`] puts the blocks relative to the target.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Target {
    /// Before the target, as its sibling.
    Before(BlockId),
    /// After the target, as its sibling.
    After(BlockId),
    /// First children of the target.
    FirstChild(BlockId),
    /// Last children of the target.
    LastChild(BlockId),
}

/// A file to attach (see [`Cmd::ImportAssets`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAsset {
    /// Where it is written (`assets/<name>`).
    pub path: GraphPath,
    /// Content.
    pub bytes: std::sync::Arc<[u8]>,
    /// The Markdown link inserted in the block.
    pub link: String,
}

/// A user intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    /// Replace the text of a block.
    SetText {
        /// Block.
        id: BlockId,
        /// New text.
        text: String,
    },
    /// New block after `after` as its sibling.
    InsertSibling {
        /// Reference block.
        after: BlockId,
        /// Text of the new block.
        text: String,
    },
    /// New last child of `parent`, or a new last root of `page` when `parent` is `None`.
    InsertChild {
        /// Page.
        page: PageKey,
        /// Parent.
        parent: Option<BlockId>,
        /// Text of the new block.
        text: String,
    },
    /// Delete blocks with their subtrees.
    DeleteBlocks {
        /// Selected blocks.
        ids: Vec<BlockId>,
    },
    /// Make the selected siblings last children of their previous sibling.
    Indent {
        /// Selected blocks (contiguous siblings).
        ids: Vec<BlockId>,
    },
    /// Move the selected siblings after their parent; following siblings become children of the
    /// last moved block (Logseq's default "direct" outdent).
    Outdent {
        /// Selected blocks (contiguous siblings).
        ids: Vec<BlockId>,
    },
    /// Move blocks next to or into a target.
    MoveBlocks {
        /// Selected blocks.
        ids: Vec<BlockId>,
        /// Destination.
        target: Target,
    },
    /// Collapse or expand (`collapsed:: true` in the file).
    SetCollapsed {
        /// Blocks.
        ids: Vec<BlockId>,
        /// New state.
        collapsed: bool,
    },
    /// Set a property line.
    SetProperty {
        /// Block.
        id: BlockId,
        /// Key.
        key: String,
        /// Value.
        value: String,
    },
    /// Set a page property: `key:: value` in the un-bulleted pre-block (created when the page has
    /// none), or `key: value` inside YAML front matter. Never creates front matter.
    SetPageProperty {
        /// Page.
        page: PageKey,
        /// Key.
        key: String,
        /// Value.
        value: String,
    },
    /// Delete a page: its file is moved to `logseq/.recycle/` (BIT-US-0089).
    DeletePage {
        /// Page.
        page: PageKey,
    },
    /// Delete an asset file (an image or attachment a block points to): moved to
    /// `logseq/.recycle/`. The caller asks for confirmation first.
    DeleteAsset {
        /// Graph-relative path (`assets/x.png`).
        path: GraphPath,
    },
    /// What Enter does in a block: outdent when it is an empty last child, otherwise split at the
    /// caret (BIT-US-0032). Offsets are bytes of the block text. See [`super::split::enter_action`]
    /// for the other cases (code fence, `[[ ]]`) that the editor resolves first.
    Enter {
        /// Block.
        id: BlockId,
        /// Caret or selection.
        cursor: Range<usize>,
        /// Block the view is zoomed into: Enter on its empty last child does not outdent out of it.
        zoom_root: Option<BlockId>,
    },
    /// Split a block at the caret: `SetText(head)` + a new block with the left-trimmed tail.
    SplitBlock {
        /// Block.
        id: BlockId,
        /// Caret or selection (the selection is deleted).
        cursor: Range<usize>,
    },
    /// Shift+Enter: replace the selection by a line break inside the block.
    InsertNewline {
        /// Block.
        id: BlockId,
        /// Caret or selection.
        at: Range<usize>,
    },
    /// Enter on an empty last child: move it after its parent.
    OutdentEmptyLast {
        /// Block.
        id: BlockId,
    },
    /// Backspace at offset 0: join the block onto the previous visible block.
    MergeWithPrevious {
        /// Block.
        id: BlockId,
    },
    /// Delete at the end of the block: pull the next block (first child or next sibling) in.
    MergeNext {
        /// Block.
        id: BlockId,
    },
    /// Alt+Shift+Up/Down.
    MoveUpDown {
        /// Selected blocks (contiguous siblings).
        ids: Vec<BlockId>,
        /// Direction.
        up: bool,
    },
    /// Collapse or expand blocks that have children; leaves are ignored (arrow click, Mod+Up/Down).
    CollapseBlocks {
        /// Blocks.
        ids: Vec<BlockId>,
        /// New state.
        collapsed: bool,
    },
    /// Without a target Mod+Up/Down act on the page: collapse the deepest expanded level or
    /// expand the shallowest collapsed one.
    CollapseLevel {
        /// Page.
        page: PageKey,
        /// Collapse (true) or expand (false) one level.
        collapse: bool,
    },
    /// `t o`: collapse or expand every block that has children.
    SetAllCollapsed {
        /// Page.
        page: PageKey,
        /// New state.
        collapsed: bool,
    },
    /// Cycle the task marker of each block per `:preferred-workflow` (Mod+Enter); empty blocks are
    /// skipped.
    CycleMarker {
        /// Blocks.
        ids: Vec<BlockId>,
    },
    /// Set (`Some`) or remove (`None`) the task marker.
    SetMarker {
        /// Blocks.
        ids: Vec<BlockId>,
        /// Marker word.
        marker: Option<String>,
    },
    /// Checkbox click: `DONE` blocks go back to the workflow's first marker, the others to `DONE`.
    ToggleDone {
        /// Blocks.
        ids: Vec<BlockId>,
    },
    /// Replace a byte range of a block text (typing). Consecutive ones on one block coalesce into
    /// one undo step.
    EditText {
        /// Block.
        id: BlockId,
        /// Range to replace.
        range: Range<usize>,
        /// New text.
        inserted: String,
    },
    /// Insert block trees next to or below `target` (paste, templates; BIT-US-0037).
    InsertBlocks {
        /// Block the new ones go next to.
        target: BlockId,
        /// Force sibling (true) or first child (false); `None` = Logseq's rule.
        sibling: Option<bool>,
        /// The trees.
        blocks: Vec<super::clipboard::ClipBlock>,
        /// Keep the `id::` of the pasted blocks (cut and external paste) instead of dropping it.
        keep_uuids: bool,
    },
    /// Paste plain text: a Markdown outline becomes blocks, paragraphs become sibling blocks,
    /// anything else is inserted inline at the caret.
    PasteText {
        /// Edited block.
        target: BlockId,
        /// Caret or selection.
        cursor: Range<usize>,
        /// Clipboard text.
        text: String,
        /// Raw paste (Mod+Shift+V): always inline.
        raw: bool,
    },
    /// Save pasted or dropped files as attachments and put their links into `target` in place
    /// of `cursor`, as one undoable transaction (BIT-US-0096). The links are one per line.
    ImportAssets {
        /// Block being edited.
        target: BlockId,
        /// Caret or selection (bytes of the block text).
        cursor: Range<usize>,
        /// The files and the links that point at them.
        assets: Vec<NewAsset>,
    },
    /// Insert the template `name` in place of the `/template` trigger text `trigger` of `target`
    /// (BIT-US-0105): the trigger is removed and the expanded blocks are inserted after the
    /// block (as its children when it has visible children), or replace it when nothing else is
    /// left in it. One transaction.
    InsertTemplate {
        /// Block being edited.
        target: BlockId,
        /// Trigger text to remove (bytes of the block text).
        trigger: Range<usize>,
        /// Template name (`template:: name`).
        name: String,
        /// Values of the variables (`<% today %>` ...).
        ctx: super::lifecycle::TemplateContext,
    },
    /// Alt-drop (BIT-US-0106): a new block holding `((uuid))` of `source` at `target`, adding the
    /// `id::` to `source` in the same transaction. The source is not moved.
    DropBlockRef {
        /// The dragged block.
        source: BlockId,
        /// Where the reference block goes.
        target: Target,
    },
    /// Give a block an `id::` (no-op when it has one); the id is generated unless given.
    EnsureUuid {
        /// Block.
        id: BlockId,
        /// The uuid to use.
        uuid: Option<uuid::Uuid>,
    },
    /// Replace `range` of `target` by `((uuid))` of `referenced`, adding its `id::` in the same
    /// transaction (block-reference completion).
    InsertBlockRef {
        /// Block being edited.
        target: BlockId,
        /// Query range to replace (from `((` to the caret, including the pair).
        range: Range<usize>,
        /// The referenced block.
        referenced: BlockId,
    },
}

/// Why a command did nothing.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Refusal {
    /// A referenced block does not exist.
    #[error("unknown block {0:?}")]
    UnknownBlock(BlockId),
    /// The command would not change anything.
    #[error("nothing to do")]
    NoChange,
    /// The selection is empty.
    #[error("empty selection")]
    EmptySelection,
    /// The selected blocks are not contiguous siblings.
    #[error("selection must be contiguous siblings")]
    NotSiblings,
    /// The first selected block has no previous sibling to indent under.
    #[error("no previous sibling to indent under")]
    NoPreviousSibling,
    /// A top-level block cannot be outdented.
    #[error("block is already at the top level")]
    AlreadyTopLevel,
    /// Target lies inside the moved blocks (or is one of them).
    #[error("cannot move a block into itself")]
    TargetInsideSelection,
    /// The page is read-only.
    #[error("page is read-only")]
    ReadOnly,
    /// The path belongs to a page; delete pages with `DeletePage`.
    #[error("path is a page file, not an asset")]
    NotAnAsset,
    /// A line of the text would re-parse as a new block (e.g. `x\n- y`), so it cannot live in one
    /// block. Logseq reads an indented `- ` line as a nested bullet too.
    #[error("text has a line that starts like a list item and cannot be stored in one block")]
    Unrepresentable,
    /// The caret offset is outside the text or not on a character boundary.
    #[error("caret offset is not inside the block text")]
    BadCursor,
    /// Enter on an empty last child only outdents when the block is empty and has no next sibling.
    #[error("block is not an empty last child")]
    NotEmptyLastChild,
    /// Backspace on the first block of a page does nothing unless the block is empty.
    #[error("first block of the page cannot be merged into anything")]
    FirstBlock,
    /// A page keeps at least one block.
    #[error("the page has a single block")]
    LastBlockOfPage,
    /// Logseq refuses a merge when both blocks have children.
    #[error("both blocks have children")]
    BothHaveChildren,
    /// There is no next block to pull in.
    #[error("no next block")]
    NoNextBlock,
    /// Both blocks carry an `id::`, so one of the identities would be lost.
    #[error("both blocks have an id::")]
    BothReferenced,
    /// The block is at the page edge and cannot move further.
    #[error("block cannot move further")]
    AtEdge,
    /// Enter should move the caret instead (past the closing `]]`).
    #[error("move the caret to offset {0}")]
    MoveCaret(usize),
    /// Nothing was selected for the operation to act on.
    #[error("no block has what the command needs")]
    NothingApplicable,
}

/// A planned command: the ops and where the caret goes afterwards.
#[derive(Debug, Clone)]
pub struct Planned {
    /// The ops, to be committed as one transaction.
    pub ops: Vec<Op>,
    /// Caret after the command (restored on redo).
    pub cursor_after: Option<super::tx::CursorState>,
}

impl From<Vec<Op>> for Planned {
    fn from(ops: Vec<Op>) -> Self {
        Self {
            ops,
            cursor_after: None,
        }
    }
}

/// Plans `cmd` against `ws` without changing it.
///
/// # Errors
/// A [`Refusal`] explaining why the command is a no-op or invalid.
pub fn plan(ws: &Workspace, cmd: &Cmd) -> Result<Vec<Op>, Refusal> {
    plan_full(ws, cmd).map(|p| p.ops)
}

/// Like [`plan`], also returning where the caret goes afterwards.
///
/// # Errors
/// A [`Refusal`] explaining why the command is a no-op or invalid.
pub fn plan_full(ws: &Workspace, cmd: &Cmd) -> Result<Planned, Refusal> {
    match cmd {
        Cmd::Enter {
            id,
            cursor,
            zoom_root,
        } => super::split::enter(ws, *id, cursor, *zoom_root),
        Cmd::SplitBlock { id, cursor } => super::split::split_block(ws, *id, cursor),
        Cmd::InsertNewline { id, at } => super::split::insert_newline(ws, *id, at),
        Cmd::OutdentEmptyLast { id } => super::split::outdent_empty_last(ws, *id),
        Cmd::MergeWithPrevious { id } => super::split::merge_with_previous(ws, *id),
        Cmd::MergeNext { id } => super::split::merge_next(ws, *id),
        Cmd::MoveUpDown { ids, up } => super::outline::move_up_down(ws, ids, *up).map(Into::into),
        Cmd::CollapseBlocks { ids, collapsed } => {
            super::outline::collapse_blocks(ws, ids, *collapsed).map(Into::into)
        }
        Cmd::CollapseLevel { page, collapse } => {
            super::outline::collapse_level(ws, page, *collapse).map(Into::into)
        }
        Cmd::SetAllCollapsed { page, collapsed } => {
            super::outline::set_all_collapsed(ws, page, *collapsed).map(Into::into)
        }
        Cmd::CycleMarker { ids } => super::outline::cycle_marker(ws, ids).map(Into::into),
        Cmd::SetMarker { ids, marker } => {
            super::outline::set_marker(ws, ids, marker.as_deref()).map(Into::into)
        }
        Cmd::ToggleDone { ids } => super::outline::toggle_done(ws, ids).map(Into::into),
        Cmd::EditText {
            id,
            range,
            inserted,
        } => super::split::edit_text(ws, *id, range, inserted),
        Cmd::InsertBlocks {
            target,
            sibling,
            blocks,
            keep_uuids,
        } => super::clipboard::insert_blocks(ws, *target, *sibling, blocks, *keep_uuids),
        Cmd::PasteText {
            target,
            cursor,
            text,
            raw,
        } => super::clipboard::paste_text(ws, *target, cursor, text, *raw),
        Cmd::ImportAssets {
            target,
            cursor,
            assets,
        } => super::clipboard::import_assets(ws, *target, cursor, assets),
        Cmd::DropBlockRef { source, target } => {
            super::complete::drop_block_ref(ws, *source, *target)
        }
        Cmd::InsertTemplate {
            target,
            trigger,
            name,
            ctx,
        } => super::clipboard::insert_template(ws, *target, trigger, name, ctx),
        Cmd::EnsureUuid { id, uuid } => {
            super::complete::ensure_uuid(ws, *id, *uuid).map(Into::into)
        }
        Cmd::InsertBlockRef {
            target,
            range,
            referenced,
        } => super::complete::insert_block_ref(ws, *target, range, *referenced),
        _ => plan_basic(ws, cmd).map(Into::into),
    }
}

fn plan_basic(ws: &Workspace, cmd: &Cmd) -> Result<Vec<Op>, Refusal> {
    match cmd {
        Cmd::SetText { id, text } => set_text(ws, *id, text.clone()).map(|o| vec![o]),
        Cmd::InsertSibling { after, text } => {
            let pos = ws
                .position_of(*after)
                .ok_or(Refusal::UnknownBlock(*after))?;
            check_writable(ws, &pos.page)?;
            check_text(text)?;
            Ok(vec![Op::InsertSubtree {
                page: pos.page,
                parent: pos.parent,
                index: pos.index + 1,
                subtree: Subtree::new(ws.alloc_id(), text.clone()),
            }])
        }
        Cmd::InsertChild { page, parent, text } => {
            let p = ws.page(page).ok_or(Refusal::ReadOnly)?;
            check_writable(ws, page)?;
            check_text(text)?;
            let index = p
                .children_of(*parent)
                .ok_or(Refusal::UnknownBlock(
                    parent.unwrap_or(BlockId::from_raw(0)),
                ))?
                .len();
            Ok(vec![Op::InsertSubtree {
                page: page.clone(),
                parent: *parent,
                index,
                subtree: Subtree::new(ws.alloc_id(), text.clone()),
            }])
        }
        Cmd::DeleteBlocks { ids } => {
            let tops = top_level(ws, ids)?;
            let mut ops = Vec::new();
            for id in tops {
                let page = ws.locate(id).cloned().ok_or(Refusal::UnknownBlock(id))?;
                check_writable(ws, &page)?;
                ops.push(Op::RemoveSubtree {
                    page,
                    id,
                    captured: None,
                });
            }
            Ok(ops)
        }
        Cmd::Indent { ids } => indent(ws, ids),
        Cmd::Outdent { ids } => outdent(ws, ids),
        Cmd::MoveBlocks { ids, target } => move_blocks(ws, ids, *target),
        Cmd::SetCollapsed { ids, collapsed } => {
            let mut ops = Vec::new();
            for id in top_level_or_all(ws, ids)? {
                let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
                let new = set_collapsed(&b.text, *collapsed, CollapseMode::InFile);
                if new != b.text {
                    ops.push(set_text(ws, id, new)?);
                }
            }
            if ops.is_empty() {
                return Err(Refusal::NoChange);
            }
            Ok(ops)
        }
        Cmd::SetPageProperty { page, key, value } => set_page_property(ws, page, key, value),
        Cmd::DeletePage { page } => {
            check_writable(ws, page)?;
            Ok(vec![Op::DeletePage {
                page: page.clone(),
                captured: None,
            }])
        }
        Cmd::DeleteAsset { path } => {
            if ws.page_for_path(path).is_some() {
                return Err(Refusal::NotAnAsset);
            }
            Ok(vec![Op::DeleteAsset { path: path.clone() }])
        }
        Cmd::SetProperty { id, key, value } => {
            let b = ws.block(*id).ok_or(Refusal::UnknownBlock(*id))?;
            let new = set_property(&b.text, key, value);
            set_text(ws, *id, new).map(|o| vec![o])
        }
        _ => Err(Refusal::NothingApplicable),
    }
}

fn set_page_property(
    ws: &Workspace,
    page: &PageKey,
    key: &str,
    value: &str,
) -> Result<Vec<Op>, Refusal> {
    check_writable(ws, page)?;
    let p = ws.page(page).ok_or(Refusal::ReadOnly)?;
    let before = p.preamble.clone();
    let current = before.as_deref().filter(|t| !t.trim().is_empty());
    let after = match current {
        Some(t) if t.trim_start().starts_with("---") => set_front_matter_property(t, key, value),
        Some(t) => set_property(t, key, value),
        None => set_property("", key, value),
    };
    if before.as_deref() == Some(after.as_str()) {
        return Err(Refusal::NoChange);
    }
    Ok(vec![Op::SetPreamble {
        page: page.clone(),
        before,
        after: Some(after),
    }])
}

pub(super) fn check_text(text: &str) -> Result<(), Refusal> {
    if super::model::text_is_representable(text) {
        Ok(())
    } else {
        Err(Refusal::Unrepresentable)
    }
}

pub(super) fn check_writable(ws: &Workspace, page: &PageKey) -> Result<(), Refusal> {
    match ws.page(page) {
        Some(p) if !p.read_only => Ok(()),
        _ => Err(Refusal::ReadOnly),
    }
}

pub(super) fn set_text(ws: &Workspace, id: BlockId, after: String) -> Result<Op, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    if let Some(page) = ws.locate(id) {
        check_writable(ws, page)?;
    }
    if b.text == after {
        return Err(Refusal::NoChange);
    }
    let loaded = b
        .origin
        .as_ref()
        .is_some_and(|o| o.text_hash == super::model::text_hash(&after));
    if !loaded {
        check_text(&after)?;
    }
    Ok(Op::SetText {
        id,
        before: b.text.clone(),
        after,
    })
}

/// Selected blocks without those that have a selected ancestor, in document order of the page.
fn top_level_or_all(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<BlockId>, Refusal> {
    if ids.is_empty() {
        return Err(Refusal::EmptySelection);
    }
    for id in ids {
        ws.block(*id).ok_or(Refusal::UnknownBlock(*id))?;
    }
    Ok(ids.to_vec())
}

pub(super) fn top_level(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<BlockId>, Refusal> {
    let all = top_level_or_all(ws, ids)?;
    Ok(all
        .iter()
        .copied()
        .filter(|id| {
            !all.iter()
                .any(|other| other != id && ws.is_within(*other, *id))
        })
        .collect())
}

/// Validates that `ids` are contiguous siblings; returns their slot (first index) in order.
pub(super) fn sibling_run(
    ws: &Workspace,
    ids: &[BlockId],
) -> Result<(Position, Vec<BlockId>), Refusal> {
    let tops = top_level(ws, ids)?;
    let first = ws
        .position_of(tops[0])
        .ok_or(Refusal::UnknownBlock(tops[0]))?;
    check_writable(ws, &first.page)?;
    let page = ws.page(&first.page).ok_or(Refusal::ReadOnly)?;
    let siblings = page
        .children_of(first.parent)
        .ok_or(Refusal::UnknownBlock(tops[0]))?;
    let mut sorted: Vec<(usize, BlockId)> = Vec::new();
    for id in &tops {
        let idx = siblings
            .iter()
            .position(|s| s == id)
            .ok_or(Refusal::NotSiblings)?;
        sorted.push((idx, *id));
    }
    sorted.sort_unstable();
    if sorted.windows(2).any(|w| w[1].0 != w[0].0 + 1) {
        return Err(Refusal::NotSiblings);
    }
    let run: Vec<BlockId> = sorted.iter().map(|(_, id)| *id).collect();
    let pos = Position {
        page: first.page,
        parent: first.parent,
        index: sorted[0].0,
    };
    Ok((pos, run))
}

fn indent(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<Op>, Refusal> {
    let (pos, run) = sibling_run(ws, ids)?;
    if pos.index == 0 {
        return Err(Refusal::NoPreviousSibling);
    }
    let page = ws.page(&pos.page).ok_or(Refusal::ReadOnly)?;
    let sibs = page.children_of(pos.parent).ok_or(Refusal::ReadOnly)?;
    let prev = sibs[pos.index - 1];
    let prev_block = ws.block(prev).ok_or(Refusal::UnknownBlock(prev))?;
    let n = prev_block.children.len();
    let mut ops = Vec::new();
    for (k, id) in run.iter().enumerate() {
        ops.push(Op::Move {
            id: *id,
            from: None,
            to: Position {
                page: pos.page.clone(),
                parent: Some(prev),
                index: n + k,
            },
        });
    }
    // Expanding the new parent keeps the indented blocks visible (Logseq behaviour).
    let expanded = set_collapsed(&prev_block.text, false, CollapseMode::InFile);
    if expanded != prev_block.text {
        ops.push(Op::SetText {
            id: prev,
            before: prev_block.text.clone(),
            after: expanded,
        });
    }
    Ok(ops)
}

fn outdent(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<Op>, Refusal> {
    let (pos, run) = sibling_run(ws, ids)?;
    let parent = pos.parent.ok_or(Refusal::AlreadyTopLevel)?;
    let ppos = ws
        .position_of(parent)
        .ok_or(Refusal::UnknownBlock(parent))?;
    let page = ws.page(&pos.page).ok_or(Refusal::ReadOnly)?;
    let sibs = page.children_of(pos.parent).ok_or(Refusal::ReadOnly)?;
    let following: Vec<BlockId> = sibs[pos.index + run.len()..].to_vec();
    let mut ops = Vec::new();
    for (k, id) in run.iter().enumerate() {
        ops.push(Op::Move {
            id: *id,
            from: None,
            to: Position {
                page: pos.page.clone(),
                parent: ppos.parent,
                index: ppos.index + 1 + k,
            },
        });
    }
    // Logical outdenting (`:editor/logical-outdenting?`) leaves the following siblings in place.
    if !ws.settings().logical_outdenting
        && let Some(last) = run.last()
    {
        let existing = ws.block(*last).map_or(0, |b| b.children.len());
        for (j, id) in following.iter().enumerate() {
            ops.push(Op::Move {
                id: *id,
                from: None,
                to: Position {
                    page: pos.page.clone(),
                    parent: Some(*last),
                    index: existing + j,
                },
            });
        }
    }
    Ok(ops)
}

pub(super) fn move_blocks(
    ws: &Workspace,
    ids: &[BlockId],
    target: Target,
) -> Result<Vec<Op>, Refusal> {
    let tops = top_level(ws, ids)?;
    let anchor = match target {
        Target::Before(t) | Target::After(t) | Target::FirstChild(t) | Target::LastChild(t) => t,
    };
    if tops.iter().any(|id| ws.is_within(*id, anchor)) {
        return Err(Refusal::TargetInsideSelection);
    }
    let apos = ws
        .position_of(anchor)
        .ok_or(Refusal::UnknownBlock(anchor))?;
    check_writable(ws, &apos.page)?;
    for id in &tops {
        if let Some(p) = ws.locate(*id) {
            check_writable(ws, p)?;
        }
    }
    let (page, parent) = match target {
        Target::Before(_) | Target::After(_) => (apos.page.clone(), apos.parent),
        Target::FirstChild(_) | Target::LastChild(_) => (apos.page.clone(), Some(anchor)),
    };
    let dest = ws.page(&page).ok_or(Refusal::ReadOnly)?;
    // Simulate the destination list while the moves run: `Position::index` counts the siblings
    // without the moved block, and the other selected blocks may still sit among them.
    let mut cur: Vec<BlockId> = dest.children_of(parent).cloned().unwrap_or_default();
    let mut ops = Vec::with_capacity(tops.len());
    let mut prev: Option<BlockId> = None;
    for id in &tops {
        cur.retain(|c| c != id);
        let index = match (prev, target) {
            (Some(p), _) => cur
                .iter()
                .position(|c| *c == p)
                .map_or(cur.len(), |i| i + 1),
            (None, Target::Before(t)) => cur.iter().position(|c| *c == t).unwrap_or(cur.len()),
            (None, Target::After(t)) => cur
                .iter()
                .position(|c| *c == t)
                .map_or(cur.len(), |i| i + 1),
            (None, Target::FirstChild(_)) => 0,
            (None, Target::LastChild(_)) => cur.len(),
        };
        cur.insert(index, *id);
        ops.push(Op::Move {
            id: *id,
            from: None,
            to: Position {
                page: page.clone(),
                parent,
                index,
            },
        });
        prev = Some(*id);
    }
    Ok(ops)
}
