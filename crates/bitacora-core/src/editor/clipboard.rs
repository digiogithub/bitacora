//! Copy, cut and paste of block subtrees and Markdown text (BIT-US-0037, BIT-SP-0004.R13).
//!
//! Export follows Logseq's clipboard contents: plain Markdown (tab-indented from depth 0, `id::`
//! lines stripped), HTML, and a private payload that keeps the `id::` lines so a cut can be pasted
//! with its identities. Paste classification follows `paste.cljs` (`markdown-blocks?`,
//! `paste-segmented-text`, `paste-copied-blocks-or-text`) and `editor.cljs` `paste-blocks`
//! (`replace-empty-target?`, `keep-uuid?`); written from scratch (ADR-015).

use std::collections::HashSet;
use std::ops::Range;

use bitacora_markdown::Document;
use bitacora_markdown::edit::identity::block_id;
use bitacora_markdown::edit::properties::remove_property;
use uuid::Uuid;

use super::cmd::{Cmd, Planned, Refusal, check_text, check_writable};
use super::model::{BlockId, Subtree};
use super::op::Op;
use super::text::{has_visible_children, is_blank};
use super::tx::{CommitError, CursorState, Transaction};
use super::workspace::Workspace;

/// A detached block tree as it travels through the clipboard (text includes `id::` lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipBlock {
    /// Logical block text.
    pub text: String,
    /// Child trees.
    pub children: Vec<ClipBlock>,
}

impl ClipBlock {
    /// A block without children.
    #[must_use]
    pub fn leaf(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            children: Vec::new(),
        }
    }
}

/// Everything a copy puts on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardPayload {
    /// `text/plain`: Markdown, tab-indented, `id::` stripped.
    pub text: String,
    /// `text/html`: nested lists.
    pub html: String,
    /// Private MIME payload (`application/x-bitacora-blocks`): the Markdown with `id::` kept,
    /// after a one-line header that says whether it was a cut.
    pub private: String,
}

/// MIME type of [`ClipboardPayload::private`].
pub const PRIVATE_MIME: &str = "application/x-bitacora-blocks";

const HEADER_COPY: &str = "bitacora-blocks/1 copy\n";
const HEADER_CUT: &str = "bitacora-blocks/1 cut\n";

fn snapshot(ws: &Workspace, id: BlockId) -> Option<ClipBlock> {
    let b = ws.block(id)?;
    Some(ClipBlock {
        text: b.text.clone(),
        children: b.children.iter().filter_map(|c| snapshot(ws, *c)).collect(),
    })
}

/// Selected blocks as trees: top-level blocks only, in document order.
///
/// # Errors
/// [`Refusal::EmptySelection`] or [`Refusal::UnknownBlock`].
pub fn selection_trees(ws: &Workspace, ids: &[BlockId]) -> Result<Vec<ClipBlock>, Refusal> {
    if ids.is_empty() {
        return Err(Refusal::EmptySelection);
    }
    let mut keyed: Vec<(crate::graph::PageKey, usize, BlockId)> = Vec::new();
    for id in ids {
        let page = ws.locate(*id).ok_or(Refusal::UnknownBlock(*id))?;
        let p = ws.page(page).ok_or(Refusal::UnknownBlock(*id))?;
        if ids
            .iter()
            .any(|o| o != id && ws.is_within(*o, *id) && ws.locate(*o) == Some(page))
        {
            continue;
        }
        let at = p.dfs().iter().position(|b| b == id).unwrap_or(usize::MAX);
        keyed.push((page.clone(), at, *id));
    }
    keyed.sort();
    keyed.dedup();
    Ok(keyed
        .into_iter()
        .filter_map(|(_, _, id)| snapshot(ws, id))
        .collect())
}

fn write_md(out: &mut String, blocks: &[ClipBlock], depth: usize, strip_ids: bool) {
    for b in blocks {
        let text = if strip_ids {
            strip_id_line(&b.text)
        } else {
            b.text.clone()
        };
        let indent = "\t".repeat(depth);
        for (i, line) in text.split('\n').enumerate() {
            if i == 0 {
                out.push_str(&indent);
                out.push_str("- ");
            } else if !line.is_empty() {
                out.push_str(&indent);
                out.push_str("  ");
            }
            out.push_str(line);
            out.push('\n');
        }
        write_md(out, &b.children, depth + 1, strip_ids);
    }
}

fn strip_id_line(text: &str) -> String {
    if block_id(text).is_some() {
        remove_property(text, "id")
    } else {
        text.to_owned()
    }
}

/// Serializes block trees as Logseq Markdown (tab-indented from depth 0).
#[must_use]
pub fn to_markdown(blocks: &[ClipBlock], strip_ids: bool) -> String {
    let mut out = String::new();
    write_md(&mut out, blocks, 0, strip_ids);
    out
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn write_html(out: &mut String, blocks: &[ClipBlock]) {
    out.push_str("<ul>");
    for b in blocks {
        out.push_str("<li>");
        out.push_str(&escape_html(&strip_id_line(&b.text)).replace('\n', "<br>"));
        if !b.children.is_empty() {
            write_html(out, &b.children);
        }
        out.push_str("</li>");
    }
    out.push_str("</ul>");
}

/// Builds the clipboard contents for trees.
#[must_use]
pub fn payload_of(blocks: &[ClipBlock], cut: bool) -> ClipboardPayload {
    let mut html = String::new();
    write_html(&mut html, blocks);
    let mut private = String::from(if cut { HEADER_CUT } else { HEADER_COPY });
    private.push_str(&to_markdown(blocks, false));
    ClipboardPayload {
        text: to_markdown(blocks, true),
        html,
        private,
    }
}

/// Copy: the clipboard contents for the selection.
///
/// # Errors
/// [`Refusal::EmptySelection`] or [`Refusal::UnknownBlock`].
pub fn export_blocks(
    ws: &Workspace,
    ids: &[BlockId],
    cut: bool,
) -> Result<ClipboardPayload, Refusal> {
    Ok(payload_of(&selection_trees(ws, ids)?, cut))
}

/// Reads a private payload: `(cut, trees)`; `None` when `s` is not one.
#[must_use]
pub fn parse_private(s: &str) -> Option<(bool, Vec<ClipBlock>)> {
    let (cut, rest) = if let Some(r) = s.strip_prefix(HEADER_CUT) {
        (true, r)
    } else {
        (false, s.strip_prefix(HEADER_COPY)?)
    };
    Some((cut, parse_outline(rest)))
}

/// Parses Markdown into block trees: bullets (`-`, `*`, `+`, any indentation), bare ATX headings and
/// text before the first bullet (one block). CRLF is normalised.
#[must_use]
pub fn parse_outline(text: &str) -> Vec<ClipBlock> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let doc = Document::parse(text.into_bytes());
    let mut flat: Vec<(usize, String)> = Vec::new();
    if let Some(pre) = doc.pre_block_text() {
        let t = pre.trim();
        if !t.is_empty() {
            flat.push((1, t.to_owned()));
        }
    }
    for (i, node) in doc.blocks.iter().enumerate() {
        let content = doc
            .block_content(i)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default();
        flat.push((node.depth().max(1), content));
    }
    build_forest(&flat)
}

fn build_forest(flat: &[(usize, String)]) -> Vec<ClipBlock> {
    // Parent index stack by depth; depth jumps (a level skipped) nest under the last block.
    let mut roots: Vec<ClipBlock> = Vec::new();
    let mut path: Vec<usize> = Vec::new(); // child index at each open level
    let mut depths: Vec<usize> = Vec::new();
    for (depth, text) in flat {
        while depths.last().is_some_and(|d| d >= depth) {
            depths.pop();
            path.pop();
        }
        let block = ClipBlock::leaf(text.clone());
        let list = children_at(&mut roots, &path);
        list.push(block);
        path.push(list.len() - 1);
        depths.push(*depth);
    }
    roots
}

fn children_at<'a>(roots: &'a mut Vec<ClipBlock>, path: &[usize]) -> &'a mut Vec<ClipBlock> {
    let mut list = roots;
    for i in path {
        list = &mut list[*i].children;
    }
    list
}

/// `^\s*([-+*]|#+)\s+` on any line (Logseq's `markdown-blocks?`).
#[must_use]
pub fn looks_like_markdown_blocks(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        let rest = if let Some(r) = t.strip_prefix(['-', '+', '*']) {
            r
        } else if t.starts_with('#') {
            t.trim_start_matches('#')
        } else {
            return false;
        };
        rest.starts_with(char::is_whitespace)
    })
}

/// Splits on blank-line runs into paragraphs, each prefixed with `- ` unless it already starts
/// like a bullet (`paste-segmented-text`).
fn segment_paragraphs(text: &str) -> Vec<ClipBlock> {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                paragraphs.push(cur.join("\n"));
                cur.clear();
            }
        } else {
            cur.push(line);
        }
    }
    if !cur.is_empty() {
        paragraphs.push(cur.join("\n"));
    }
    let joined: Vec<String> = paragraphs
        .iter()
        .map(|p| {
            let p = p.trim();
            if p.trim_start().starts_with("- ") {
                p.to_owned()
            } else {
                format!("- {p}")
            }
        })
        .collect();
    parse_outline(&joined.join("\n"))
}

/// How a plain-text paste is interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteKind {
    /// A Markdown outline or blank-line separated paragraphs: these blocks.
    Blocks(Vec<ClipBlock>),
    /// Inline text at the caret.
    Inline(String),
}

/// Classifies clipboard text (CRLF normalised): outline markers first, then blank-line separated
/// paragraphs, otherwise inline. `raw` (Mod+Shift+V) is always inline.
#[must_use]
pub fn classify_paste(text: &str, raw: bool) -> PasteKind {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if raw {
        return PasteKind::Inline(text);
    }
    if looks_like_markdown_blocks(&text) {
        return PasteKind::Blocks(parse_outline(&text));
    }
    if text.trim().contains("\n\n") {
        let blocks = segment_paragraphs(&text);
        if blocks.len() > 1 {
            return PasteKind::Blocks(blocks);
        }
    }
    PasteKind::Inline(text)
}

struct Builder<'a> {
    ws: &'a Workspace,
    keep: bool,
    used: HashSet<Uuid>,
    last: Option<(BlockId, usize)>,
}

impl Builder<'_> {
    fn subtree(&mut self, c: &ClipBlock) -> Result<Subtree, Refusal> {
        let mut text = c.text.clone();
        if let Some(u) = block_id(&text) {
            let free = self.keep && self.ws.uuid_count(&u) == 0 && self.used.insert(u);
            if !free {
                text = remove_property(&text, "id");
            }
        }
        check_text(&text)?;
        let id = self.ws.alloc_id();
        self.last = Some((id, text.len()));
        let mut st = Subtree::new(id, text);
        for ch in &c.children {
            st.children.push(self.subtree(ch)?);
        }
        Ok(st)
    }
}

pub(super) fn insert_blocks(
    ws: &Workspace,
    target: BlockId,
    sibling: Option<bool>,
    blocks: &[ClipBlock],
    keep_uuids: bool,
) -> Result<Planned, Refusal> {
    if blocks.is_empty() {
        return Err(Refusal::EmptySelection);
    }
    let t = ws.block(target).ok_or(Refusal::UnknownBlock(target))?;
    let pos = ws
        .position_of(target)
        .ok_or(Refusal::UnknownBlock(target))?;
    check_writable(ws, &pos.page)?;
    let mut b = Builder {
        ws,
        keep: keep_uuids,
        used: HashSet::new(),
        last: None,
    };
    let mut trees = Vec::with_capacity(blocks.len());
    for c in blocks {
        trees.push(b.subtree(c)?);
    }
    let replace = is_blank(&t.text) && t.uuid.is_none() && t.children.is_empty();
    let mut ops = Vec::new();
    let (parent, index) = if replace {
        (pos.parent, pos.index)
    } else {
        let as_sibling = sibling.unwrap_or(!has_visible_children(t));
        if as_sibling {
            (pos.parent, pos.index + 1)
        } else {
            (Some(target), 0)
        }
    };
    for (k, st) in trees.into_iter().enumerate() {
        ops.push(Op::InsertSubtree {
            page: pos.page.clone(),
            parent,
            index: index + k,
            subtree: st,
        });
    }
    if replace {
        ops.push(Op::RemoveSubtree {
            page: pos.page,
            id: target,
            captured: None,
        });
    }
    let cursor_after = b.last.map(|(block, end)| CursorState {
        block,
        selection: end..end,
    });
    Ok(Planned { ops, cursor_after })
}

pub(super) fn paste_text(
    ws: &Workspace,
    target: BlockId,
    cursor: &Range<usize>,
    text: &str,
    raw: bool,
) -> Result<Planned, Refusal> {
    match classify_paste(text, raw) {
        PasteKind::Blocks(blocks) => insert_blocks(ws, target, None, &blocks, true),
        PasteKind::Inline(t) => super::split::edit_text(ws, target, cursor, &t),
    }
}

impl Workspace {
    /// Cut: the clipboard contents for the selection and its deletion, as one transaction.
    ///
    /// # Errors
    /// [`CommitError`] when the selection is invalid or the deletion fails (nothing changes).
    pub fn cut_blocks(
        &mut self,
        label: &'static str,
        ids: &[BlockId],
    ) -> Result<(ClipboardPayload, Transaction), CommitError> {
        let payload = export_blocks(self, ids, true)?;
        let tx = self.run(label, &Cmd::DeleteBlocks { ids: ids.to_vec() })?;
        Ok((payload, tx))
    }

    /// Paste a private payload: identities are kept after a cut and dropped after a copy.
    ///
    /// # Errors
    /// [`CommitError`] when the target is invalid.
    pub fn paste_private(
        &mut self,
        label: &'static str,
        target: BlockId,
        private: &str,
    ) -> Result<Option<Transaction>, CommitError> {
        let Some((cut, blocks)) = parse_private(private) else {
            return Ok(None);
        };
        self.run(
            label,
            &Cmd::InsertBlocks {
                target,
                sibling: None,
                blocks,
                keep_uuids: cut,
            },
        )
        .map(Some)
    }
}
