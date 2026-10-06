//! Enter / Shift+Enter / Backspace / Delete: splitting and merging blocks with Logseq's rules
//! (BIT-US-0032, BIT-US-0033; `docs/analysis/logseq/04-editor-outliner-operations.md` §3,
//! behaviour read from `editor.cljs` `keydown-new-block`, `insert-new-block!`, `delete-block!`,
//! `delete-concat`; written from scratch, ADR-015).
//!
//! Offsets are byte offsets into the block text on char boundaries. The text is the full block
//! text (properties included); editors that project hidden properties out of the buffer map their
//! offsets back first.

use std::ops::Range;

use bitacora_markdown::edit::properties::set_property;
use uuid::Uuid;

use super::cmd::{Planned, Refusal, check_text, check_writable, set_text};
use super::model::{BlockId, Position, Subtree, text_is_isolated};
use super::op::Op;
use super::text::{
    add_properties, append_to_body, has_visible_children, is_blank, is_collapsed, left_of,
    prev_visible, property_lines, right_of, strip_properties, take_hidden,
};
use super::tx::CursorState;
use super::workspace::Workspace;

fn caret(block: BlockId, at: usize) -> Option<CursorState> {
    Some(CursorState {
        block,
        selection: at..at,
    })
}

fn check_cursor(text: &str, r: &Range<usize>) -> Result<(), Refusal> {
    if r.start <= r.end
        && r.end <= text.len()
        && text.is_char_boundary(r.start)
        && text.is_char_boundary(r.end)
    {
        Ok(())
    } else {
        Err(Refusal::BadCursor)
    }
}

/// What Enter should do at this caret.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EnterAction {
    /// Empty last child: outdent ([`Cmd::OutdentEmptyLast`](super::cmd::Cmd::OutdentEmptyLast)).
    Outdent,
    /// Split the block ([`Cmd::SplitBlock`](super::cmd::Cmd::SplitBlock)).
    Split,
    /// Inside a code fence or `#+BEGIN_` block: insert a line break
    /// ([`Cmd::InsertNewline`](super::cmd::Cmd::InsertNewline)).
    Newline,
    /// Inside `[[page]]`: move the caret to this offset (just past `]]`) without splitting.
    MoveCaret(usize),
}

/// True when `offset` lies inside an unclosed fenced code block or `#+BEGIN_` block.
fn in_fenced_region(text: &str, offset: usize) -> bool {
    let mut fence = false;
    let mut begin = false;
    for line in text[..offset].split('\n') {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        } else if !fence {
            let up = t.to_ascii_uppercase();
            if up.starts_with("#+BEGIN_") {
                begin = true;
            } else if up.starts_with("#+END_") {
                begin = false;
            }
        }
    }
    fence || begin
}

/// The offset just past the `]]` that closes the `[[` the caret is inside, when there is one.
fn page_ref_end(text: &str, offset: usize) -> Option<usize> {
    let open = text[..offset].rfind("[[")?;
    if text[open..offset].contains("]]") {
        return None;
    }
    let close = text[offset..].find("]]")?;
    if text[offset..offset + close].contains("[[") {
        return None;
    }
    Some(offset + close + 2)
}

/// Decides what Enter does for `id` at `cursor`, in Logseq's precedence: code fence and page
/// reference first, then "empty last child outdents", else split.
///
/// # Errors
/// [`Refusal`] for an unknown block or an invalid caret.
pub fn enter_action(
    ws: &Workspace,
    id: BlockId,
    cursor: &Range<usize>,
    zoom_root: Option<BlockId>,
) -> Result<EnterAction, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    check_cursor(&b.text, cursor)?;
    if in_fenced_region(&b.text, cursor.start) {
        return Ok(EnterAction::Newline);
    }
    if cursor.start == cursor.end
        && let Some(end) = page_ref_end(&b.text, cursor.start)
    {
        return Ok(EnterAction::MoveCaret(end));
    }
    let pos = ws.position_of(id).ok_or(Refusal::UnknownBlock(id))?;
    if pos.parent.is_some()
        && pos.parent != zoom_root
        && is_blank(&b.text)
        && right_of(ws, id).is_none()
    {
        return Ok(EnterAction::Outdent);
    }
    Ok(EnterAction::Split)
}

pub(super) fn enter(
    ws: &Workspace,
    id: BlockId,
    cursor: &Range<usize>,
    zoom_root: Option<BlockId>,
) -> Result<Planned, Refusal> {
    match enter_action(ws, id, cursor, zoom_root)? {
        EnterAction::Outdent => outdent_empty_last(ws, id),
        EnterAction::Split => split_block(ws, id, cursor),
        EnterAction::Newline => insert_newline(ws, id, cursor),
        EnterAction::MoveCaret(at) => Err(Refusal::MoveCaret(at)),
    }
}

/// True when splitting at `cursor` puts an empty block *before* the original (caret at the start
/// of text with text after it).
#[must_use]
pub fn splits_before(text: &str, cursor: &Range<usize>) -> bool {
    check_cursor(text, cursor).is_ok()
        && is_blank(&text[..cursor.start])
        && !is_blank(text[cursor.end..].trim_start())
}

pub(super) fn split_block(
    ws: &Workspace,
    id: BlockId,
    cursor: &Range<usize>,
) -> Result<Planned, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    let pos = ws.position_of(id).ok_or(Refusal::UnknownBlock(id))?;
    check_writable(ws, &pos.page)?;
    check_cursor(&b.text, cursor)?;
    let text = b.text.as_str();
    let tail_all = text[cursor.end..].trim_start();

    if splits_before(text, cursor) {
        // Empty block before; the original (with its id and properties) keeps the text.
        let new_id = ws.alloc_id();
        let mut ops = Vec::new();
        if cursor.start != cursor.end {
            ops.push(set_text(ws, id, tail_all.to_owned())?);
        }
        ops.push(Op::InsertSubtree {
            page: pos.page,
            parent: pos.parent,
            index: pos.index,
            subtree: Subtree::new(new_id, ""),
        });
        return Ok(Planned {
            ops,
            cursor_after: caret(id, 0),
        });
    }

    // Hidden properties (`id::`, `collapsed::`) stay with the original block.
    let (tail, hidden) = take_hidden(tail_all);
    let head = add_properties(&text[..cursor.start], &hidden);
    check_text(&tail)?;
    if !text_is_isolated(&head) || !text_is_isolated(&tail) {
        // e.g. the caret is inside a code fence: both halves would swallow their neighbours.
        return Err(Refusal::Unrepresentable);
    }
    let new_id = ws.alloc_id();
    let mut ops = Vec::new();
    if head != text {
        ops.push(set_text(ws, id, head)?);
    }
    let (parent, index) = if has_visible_children(b) {
        (Some(id), 0)
    } else {
        (pos.parent, pos.index + 1)
    };
    ops.push(Op::InsertSubtree {
        page: pos.page,
        parent,
        index,
        subtree: Subtree::new(new_id, tail),
    });
    Ok(Planned {
        ops,
        cursor_after: caret(new_id, 0),
    })
}

pub(super) fn insert_newline(
    ws: &Workspace,
    id: BlockId,
    at: &Range<usize>,
) -> Result<Planned, Refusal> {
    edit_text(ws, id, at, "\n")
}

/// Typing: replaces `range` of the block text. The planned op is an [`Op::EditText`].
pub(super) fn edit_text(
    ws: &Workspace,
    id: BlockId,
    range: &Range<usize>,
    inserted: &str,
) -> Result<Planned, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    let page = ws.locate(id).ok_or(Refusal::UnknownBlock(id))?;
    check_writable(ws, page)?;
    check_cursor(&b.text, range)?;
    let removed = &b.text[range.clone()];
    if removed == inserted {
        return Err(Refusal::NoChange);
    }
    let mut new = String::with_capacity(b.text.len() + inserted.len());
    new.push_str(&b.text[..range.start]);
    new.push_str(inserted);
    new.push_str(&b.text[range.end..]);
    check_text(&new)?;
    Ok(Planned {
        ops: vec![Op::EditText {
            id,
            range: range.clone(),
            removed: removed.to_owned(),
            inserted: inserted.to_owned(),
        }],
        cursor_after: caret(id, range.start + inserted.len()),
    })
}

pub(super) fn outdent_empty_last(ws: &Workspace, id: BlockId) -> Result<Planned, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    let pos = ws.position_of(id).ok_or(Refusal::UnknownBlock(id))?;
    check_writable(ws, &pos.page)?;
    let parent = pos.parent.ok_or(Refusal::AlreadyTopLevel)?;
    if !is_blank(&b.text) || right_of(ws, id).is_some() {
        return Err(Refusal::NotEmptyLastChild);
    }
    let ppos = ws
        .position_of(parent)
        .ok_or(Refusal::UnknownBlock(parent))?;
    Ok(Planned {
        ops: vec![Op::Move {
            id,
            from: None,
            to: Position {
                page: pos.page,
                parent: ppos.parent,
                index: ppos.index + 1,
            },
        }],
        cursor_after: caret(id, 0),
    })
}

/// Text the survivor of a merge ends up with: `keep` with `addition` appended to its body, and
/// the removed block's `id::` (and other properties) when the survivor has none of its own.
/// Returns the text and the junction offset.
fn merged_text(
    keep: &str,
    keep_uuid: Option<Uuid>,
    removed: &str,
    removed_uuid: Option<Uuid>,
) -> Result<(String, usize), Refusal> {
    if keep_uuid.is_some() && removed_uuid.is_some() {
        return Err(Refusal::BothReferenced);
    }
    let addition = strip_properties(removed);
    let (mut text, junction) = append_to_body(keep, &addition);
    if let Some(u) = removed_uuid {
        // The referenced block's identity survives: the survivor takes its id and properties.
        text = set_property(&text, "id", &u.hyphenated().to_string());
        let others: Vec<(String, String)> = property_lines(removed)
            .into_iter()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("id"))
            .collect();
        text = add_properties(&text, &others);
    }
    Ok((text, junction))
}

pub(super) fn merge_with_previous(ws: &Workspace, id: BlockId) -> Result<Planned, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    let pos = ws.position_of(id).ok_or(Refusal::UnknownBlock(id))?;
    check_writable(ws, &pos.page)?;
    let page = ws.page(&pos.page).ok_or(Refusal::ReadOnly)?;

    let Some(prev_id) = prev_visible(ws, id) else {
        // First block of the page: only an empty block can go.
        if !is_blank(&b.text) {
            return Err(Refusal::FirstBlock);
        }
        if page.blocks.len() <= 1 {
            return Err(Refusal::LastBlockOfPage);
        }
        let mut ops: Vec<Op> = b
            .children
            .iter()
            .enumerate()
            .map(|(k, c)| Op::Move {
                id: *c,
                from: None,
                to: Position {
                    page: pos.page.clone(),
                    parent: pos.parent,
                    index: pos.index + 1 + k,
                },
            })
            .collect();
        ops.push(Op::RemoveSubtree {
            page: pos.page,
            id,
            captured: None,
        });
        return Ok(ops.into());
    };

    let prev = ws.block(prev_id).ok_or(Refusal::UnknownBlock(prev_id))?;
    if !b.children.is_empty() {
        let left_has_children = left_of(ws, id)
            .and_then(|l| ws.block(l))
            .is_some_and(|l| !l.children.is_empty());
        if left_has_children || !prev.children.is_empty() {
            return Err(Refusal::BothHaveChildren);
        }
    }
    let (text, junction) = merged_text(&prev.text, prev.uuid, &b.text, b.uuid)?;
    if !text_is_isolated(&text) {
        return Err(Refusal::Unrepresentable);
    }
    let mut ops = Vec::new();
    if text != prev.text {
        ops.push(set_text(ws, prev_id, text)?);
    }
    if !b.children.is_empty() {
        ops.push(Op::AdoptChildren {
            from: id,
            to: prev_id,
            at: prev.children.len(),
            moved: Vec::new(),
            src_at: None,
        });
    }
    ops.push(Op::RemoveSubtree {
        page: pos.page,
        id,
        captured: None,
    });
    Ok(Planned {
        ops,
        cursor_after: caret(prev_id, junction),
    })
}

pub(super) fn merge_next(ws: &Workspace, id: BlockId) -> Result<Planned, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    let pos = ws.position_of(id).ok_or(Refusal::UnknownBlock(id))?;
    check_writable(ws, &pos.page)?;
    let collapsed = is_collapsed(&b.text);
    let first_child_case = !collapsed && !b.children.is_empty();
    let next_id = if first_child_case {
        b.children[0]
    } else {
        right_of(ws, id).ok_or(Refusal::NoNextBlock)?
    };
    let next = ws.block(next_id).ok_or(Refusal::UnknownBlock(next_id))?;
    if (collapsed && !b.children.is_empty() && !next.children.is_empty())
        || (first_child_case && !next.children.is_empty())
    {
        return Err(Refusal::BothHaveChildren);
    }
    let (text, junction) = merged_text(&b.text, b.uuid, &next.text, next.uuid)?;
    if !text_is_isolated(&text) {
        return Err(Refusal::Unrepresentable);
    }
    let mut ops = Vec::new();
    if text != b.text {
        ops.push(set_text(ws, id, text)?);
    }
    if !next.children.is_empty() {
        ops.push(Op::AdoptChildren {
            from: next_id,
            to: id,
            at: if first_child_case {
                0
            } else {
                b.children.len()
            },
            moved: Vec::new(),
            src_at: None,
        });
    }
    ops.push(Op::RemoveSubtree {
        page: pos.page,
        id: next_id,
        captured: None,
    });
    Ok(Planned {
        ops,
        cursor_after: caret(id, junction),
    })
}
