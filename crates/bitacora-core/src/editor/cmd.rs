//! Commands: user intents compiled into ops by pure planners
//! (`docs/design/block-editor.md` §3.2). This is the skeleton; further commands are added by the
//! editing stories.

use bitacora_markdown::edit::properties::set_property;
use bitacora_markdown::edit::state::{CollapseMode, set_collapsed};

use super::model::{BlockId, Position, Subtree};
use super::op::Op;
use super::workspace::Workspace;
use crate::graph::PageKey;

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
}

/// Plans `cmd` against `ws` without changing it.
///
/// # Errors
/// A [`Refusal`] explaining why the command is a no-op or invalid.
pub fn plan(ws: &Workspace, cmd: &Cmd) -> Result<Vec<Op>, Refusal> {
    match cmd {
        Cmd::SetText { id, text } => set_text(ws, *id, text.clone()).map(|o| vec![o]),
        Cmd::InsertSibling { after, text } => {
            let pos = ws
                .position_of(*after)
                .ok_or(Refusal::UnknownBlock(*after))?;
            check_writable(ws, &pos.page)?;
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
        Cmd::SetProperty { id, key, value } => {
            let b = ws.block(*id).ok_or(Refusal::UnknownBlock(*id))?;
            let new = set_property(&b.text, key, value);
            set_text(ws, *id, new).map(|o| vec![o])
        }
    }
}

fn check_writable(ws: &Workspace, page: &PageKey) -> Result<(), Refusal> {
    match ws.page(page) {
        Some(p) if !p.read_only => Ok(()),
        _ => Err(Refusal::ReadOnly),
    }
}

fn set_text(ws: &Workspace, id: BlockId, after: String) -> Result<Op, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    if let Some(page) = ws.locate(id) {
        check_writable(ws, page)?;
    }
    if b.text == after {
        return Err(Refusal::NoChange);
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

fn top_level(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<BlockId>, Refusal> {
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
fn sibling_run(ws: &Workspace, ids: &[BlockId]) -> Result<(Position, Vec<BlockId>), Refusal> {
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
    if let Some(last) = run.last() {
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

fn move_blocks(ws: &Workspace, ids: &[BlockId], target: Target) -> Result<Vec<Op>, Refusal> {
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
    let mut list: Vec<BlockId> = dest.children_of(parent).cloned().unwrap_or_default();
    list.retain(|c| !tops.contains(c));
    let at = match target {
        Target::Before(t) => list.iter().position(|c| *c == t).unwrap_or(list.len()),
        Target::After(t) => list
            .iter()
            .position(|c| *c == t)
            .map_or(list.len(), |i| i + 1),
        Target::FirstChild(_) => 0,
        Target::LastChild(_) => list.len(),
    };
    Ok(tops
        .iter()
        .enumerate()
        .map(|(k, id)| Op::Move {
            id: *id,
            from: None,
            to: Position {
                page: page.clone(),
                parent,
                index: at + k,
            },
        })
        .collect())
}
