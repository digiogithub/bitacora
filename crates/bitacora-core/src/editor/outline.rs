//! Outline commands: moving blocks up and down, collapsing, task markers (BIT-US-0034,
//! BIT-US-0035). Behaviour read from `outliner/core.cljs` (`move-blocks-up-down`),
//! `editor.cljs` (`set-blocks-collapsed!`, `cycle-todos!`) and `util/marker.cljs`; written from
//! scratch (ADR-015).

use bitacora_markdown::edit::state::{CollapseMode, set_collapsed, set_marker as set_marker_text};
use bitacora_markdown::tasks::head::{Marker, parse_head};

use super::cmd::{Refusal, Target, move_blocks, set_text, sibling_run};
use super::model::BlockId;
use super::op::Op;
use super::text::{has_visible_children, is_blank, is_collapsed};
use super::workspace::Workspace;
use crate::graph::PageKey;

/// Alt+Shift+Up/Down: swaps the run with its neighbour sibling; at the edge of the sibling list it
/// crosses into the neighbouring parent (last child of the previous uncle going up, first child of
/// the next uncle going down). No-op (refusal) at the edges of the page.
pub(super) fn move_up_down(ws: &Workspace, ids: &[BlockId], up: bool) -> Result<Vec<Op>, Refusal> {
    let (pos, run) = sibling_run(ws, ids)?;
    let page = ws.page(&pos.page).ok_or(Refusal::ReadOnly)?;
    let sibs = page.children_of(pos.parent).ok_or(Refusal::ReadOnly)?;
    let last = pos.index + run.len() - 1;
    let uncles = |parent: BlockId| -> Result<(usize, Vec<BlockId>), Refusal> {
        let pp = ws
            .position_of(parent)
            .ok_or(Refusal::UnknownBlock(parent))?;
        let list = page
            .children_of(pp.parent)
            .ok_or(Refusal::UnknownBlock(parent))?;
        Ok((pp.index, list.clone()))
    };
    let target = if up {
        if pos.index > 0 {
            Target::Before(sibs[pos.index - 1])
        } else {
            let parent = pos.parent.ok_or(Refusal::AtEdge)?;
            let (idx, list) = uncles(parent)?;
            if idx == 0 {
                return Err(Refusal::AtEdge);
            }
            Target::LastChild(list[idx - 1])
        }
    } else if last + 1 < sibs.len() {
        Target::After(sibs[last + 1])
    } else {
        let parent = pos.parent.ok_or(Refusal::AtEdge)?;
        let (idx, list) = uncles(parent)?;
        let next = list.get(idx + 1).ok_or(Refusal::AtEdge)?;
        Target::FirstChild(*next)
    };
    move_blocks(ws, &run, target)
}

fn unique_blocks(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<BlockId>, Refusal> {
    if ids.is_empty() {
        return Err(Refusal::EmptySelection);
    }
    let mut out: Vec<BlockId> = Vec::with_capacity(ids.len());
    for id in ids {
        ws.block(*id).ok_or(Refusal::UnknownBlock(*id))?;
        if !out.contains(id) {
            out.push(*id);
        }
    }
    Ok(out)
}

fn collapse_ops(
    ws: &Workspace,
    ids: impl IntoIterator<Item = BlockId>,
    collapsed: bool,
) -> Result<Vec<Op>, Refusal> {
    let mut ops = Vec::new();
    for id in ids {
        let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
        if b.children.is_empty() {
            continue;
        }
        let new = set_collapsed(&b.text, collapsed, CollapseMode::InFile);
        if new != b.text {
            ops.push(set_text(ws, id, new)?);
        }
    }
    if ops.is_empty() {
        return Err(Refusal::NoChange);
    }
    Ok(ops)
}

/// Arrow click / Mod+Up / Mod+Down on blocks: only blocks with children change.
pub(super) fn collapse_blocks(
    ws: &Workspace,
    ids: &[BlockId],
    collapsed: bool,
) -> Result<Vec<Op>, Refusal> {
    collapse_ops(ws, unique_blocks(ws, ids)?, collapsed)
}

/// Blocks with children that are shown (not below a collapsed block), with their depth.
fn shown_parents(ws: &Workspace, page: &PageKey) -> Result<Vec<(usize, BlockId)>, Refusal> {
    let p = ws.page(page).ok_or(Refusal::ReadOnly)?;
    let mut out = Vec::new();
    let mut stack: Vec<(usize, BlockId)> = p.roots.iter().rev().map(|r| (1, *r)).collect();
    while let Some((depth, id)) = stack.pop() {
        let Some(b) = p.block(id) else { continue };
        if b.children.is_empty() {
            continue;
        }
        out.push((depth, id));
        if has_visible_children(b) {
            stack.extend(b.children.iter().rev().map(|c| (depth + 1, *c)));
        }
    }
    Ok(out)
}

/// Mod+Up / Mod+Down without a target: collapse the deepest shown expanded level, or expand the
/// shallowest shown collapsed level.
pub(super) fn collapse_level(
    ws: &Workspace,
    page: &PageKey,
    collapse: bool,
) -> Result<Vec<Op>, Refusal> {
    super::cmd::check_writable(ws, page)?;
    let parents = shown_parents(ws, page)?;
    let wanted = |id: &BlockId| {
        ws.block(*id)
            .is_some_and(|b| is_collapsed(&b.text) != collapse)
    };
    let candidates: Vec<(usize, BlockId)> =
        parents.into_iter().filter(|(_, id)| wanted(id)).collect();
    let depth = if collapse {
        candidates.iter().map(|(d, _)| *d).max()
    } else {
        candidates.iter().map(|(d, _)| *d).min()
    }
    .ok_or(Refusal::NoChange)?;
    collapse_ops(
        ws,
        candidates
            .into_iter()
            .filter(|(d, _)| *d == depth)
            .map(|(_, id)| id),
        collapse,
    )
}

/// `t o`: every block with children, at any depth.
pub(super) fn set_all_collapsed(
    ws: &Workspace,
    page: &PageKey,
    collapsed: bool,
) -> Result<Vec<Op>, Refusal> {
    super::cmd::check_writable(ws, page)?;
    let p = ws.page(page).ok_or(Refusal::ReadOnly)?;
    collapse_ops(ws, p.dfs(), collapsed)
}

fn marker_of(text: &str) -> Option<&'static str> {
    parse_head(text).marker.map(Marker::as_str)
}

fn marker_ops(
    ws: &Workspace,
    ids: &[BlockId],
    next: impl Fn(&str) -> Option<Option<String>>,
) -> Result<Vec<Op>, Refusal> {
    let mut ops = Vec::new();
    for id in unique_blocks(ws, ids)? {
        let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
        if is_blank(&b.text) {
            continue;
        }
        let Some(marker) = next(&b.text) else {
            continue;
        };
        let new = set_marker_text(&b.text, marker.as_deref());
        if new != b.text {
            ops.push(set_text(ws, id, new)?);
        }
    }
    if ops.is_empty() {
        return Err(Refusal::NoChange);
    }
    Ok(ops)
}

/// Mod+Enter: advance each block's marker per the preferred workflow.
pub(super) fn cycle_marker(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<Op>, Refusal> {
    let wf = ws.settings().workflow;
    marker_ops(ws, ids, |t| Some(wf.next(marker_of(t)).map(str::to_owned)))
}

/// Sets or removes the marker of each block.
pub(super) fn set_marker(
    ws: &Workspace,
    ids: &[BlockId],
    marker: Option<&str>,
) -> Result<Vec<Op>, Refusal> {
    if let Some(m) = marker
        && Marker::from_word(m).is_none()
    {
        return Err(Refusal::NothingApplicable);
    }
    let m = marker.map(str::to_owned);
    marker_ops(ws, ids, |_| Some(m.clone()))
}

/// Checkbox click.
pub(super) fn toggle_done(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<Op>, Refusal> {
    let wf = ws.settings().workflow;
    marker_ops(ws, ids, |t| {
        Some(Some(if marker_of(t) == Some("DONE") {
            wf.start().to_owned()
        } else {
            "DONE".to_owned()
        }))
    })
}
