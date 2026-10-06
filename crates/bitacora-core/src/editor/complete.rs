//! Autocomplete data and insert rules for `[[`, `#` and `((`, plus `id::` generation for referenced
//! blocks (BIT-US-0038, BIT-SP-0004.R15/R16). Written from the documented behaviour of
//! `handle-last-input` / `page-on-chosen-handler` (ADR-015).
//!
//! The popup itself and its triggers live in the app. Core supplies: the [`CompletionProvider`]
//! trait (backed by the SQLite index at runtime, by [`WorkspaceProvider`] over loaded pages here),
//! candidate filtering, the text each choice inserts, on-demand page creation and the
//! `((uuid))` insertion that persists `id::` in the same transaction (ADR-006: an `id::` is written
//! only when the block is referenced).

use std::ops::Range;

use bitacora_markdown::edit::identity::ensure_block_id;
use uuid::Uuid;

use super::cmd::{Cmd, Planned, Refusal, Target, check_writable, set_text};
use super::model::{BlockId, Subtree};
use super::op::Op;
use super::tx::{CommitError, CursorState, Transaction};
use super::workspace::Workspace;

/// A page suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSuggestion {
    /// Page title.
    pub title: String,
    /// False for the trailing "New page" entry.
    pub exists: bool,
}

/// A block suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSuggestion {
    /// Title of the page holding the block.
    pub page: String,
    /// Block text (first line is enough for display).
    pub text: String,
    /// The block's `id::`, when it has one.
    pub uuid: Option<Uuid>,
    /// The loaded block, when the provider knows it.
    pub block: Option<BlockId>,
}

/// Source of suggestions. Implemented over the index at runtime; implementations must be cheap
/// enough to call per keystroke.
pub trait CompletionProvider: Send + Sync {
    /// Pages matching `query` (fuzzy), best first, at most `limit`.
    fn search_pages(&self, query: &str, limit: usize) -> Vec<PageSuggestion>;
    /// Blocks whose text matches `query` (full text), best first, at most `limit`.
    fn search_blocks(&self, query: &str, limit: usize) -> Vec<BlockSuggestion>;
}

/// Page suggestions for the popup: the provider's matches without `current_page`, followed by a
/// "New page" entry when nothing matches the query exactly (case-insensitive).
#[must_use]
pub fn page_candidates(
    provider: &dyn CompletionProvider,
    query: &str,
    current_page: &str,
    limit: usize,
) -> Vec<PageSuggestion> {
    let q = query.trim();
    let mut out: Vec<PageSuggestion> = provider
        .search_pages(q, limit + 1)
        .into_iter()
        .filter(|p| !p.title.eq_ignore_ascii_case(current_page))
        .take(limit)
        .collect();
    if !q.is_empty() && !out.iter().any(|p| p.title.eq_ignore_ascii_case(q)) {
        out.push(PageSuggestion {
            title: q.to_owned(),
            exists: false,
        });
    }
    out
}

/// Block suggestions for `((`: at most `limit` full-text matches without the edited block and its
/// ancestors.
#[must_use]
pub fn block_candidates(
    ws: &Workspace,
    provider: &dyn CompletionProvider,
    query: &str,
    editing: BlockId,
    limit: usize,
) -> Vec<BlockSuggestion> {
    let mut excluded: Vec<BlockId> = Vec::new();
    let mut excluded_uuids: Vec<Uuid> = Vec::new();
    let mut cur = Some(editing);
    while let Some(c) = cur {
        excluded.push(c);
        let Some(b) = ws.block(c) else { break };
        excluded_uuids.extend(b.uuid);
        cur = b.parent;
    }
    provider
        .search_blocks(query.trim(), limit + excluded.len())
        .into_iter()
        .filter(|s| {
            !s.block.is_some_and(|b| excluded.contains(&b))
                && !s.uuid.is_some_and(|u| excluded_uuids.contains(&u))
        })
        .take(limit)
        .collect()
}

/// [`CompletionProvider`] over the pages loaded in a workspace snapshot (tests, headless use).
#[derive(Debug)]
pub struct WorkspaceProvider<'a>(pub &'a Workspace);

fn fuzzy(hay: &str, query: &str) -> Option<usize> {
    let (h, q) = (hay.to_lowercase(), query.to_lowercase());
    if q.is_empty() {
        return Some(0);
    }
    if let Some(i) = h.find(&q) {
        return Some(i);
    }
    let mut it = h.chars();
    q.chars()
        .all(|c| it.any(|x| x == c))
        .then_some(hay.len() + 1)
}

impl CompletionProvider for WorkspaceProvider<'_> {
    fn search_pages(&self, query: &str, limit: usize) -> Vec<PageSuggestion> {
        let mut hits: Vec<(usize, String)> = self
            .0
            .pages()
            .filter_map(|p| fuzzy(&p.title, query).map(|s| (s, p.title.clone())))
            .collect();
        hits.sort();
        hits.into_iter()
            .take(limit)
            .map(|(_, title)| PageSuggestion {
                title,
                exists: true,
            })
            .collect()
    }

    fn search_blocks(&self, query: &str, limit: usize) -> Vec<BlockSuggestion> {
        let q = query.to_lowercase();
        let mut out = Vec::new();
        for p in self.0.pages() {
            for id in p.dfs() {
                let Some(b) = p.block(id) else { continue };
                if !q.is_empty() && b.text.to_lowercase().contains(&q) {
                    out.push(BlockSuggestion {
                        page: p.title.clone(),
                        text: b.text.clone(),
                        uuid: b.uuid,
                        block: Some(id),
                    });
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out
    }
}

/// What choosing a suggestion does to the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Byte range of the block text to replace (trigger through the query and an auto-paired
    /// closer).
    pub range: Range<usize>,
    /// Replacement.
    pub text: String,
    /// Caret offset in the new text.
    pub caret: usize,
    /// Title of the page to create (a virtual page, see [`Workspace::open_page`]) when the
    /// suggestion is a "New page" entry.
    pub create_page: Option<String>,
}

/// The range from the last `open` before `caret` (no line break or `close` in between) to the caret,
/// extended over a `close` that directly follows (the auto-paired closer). `None` when the caret
/// is not inside such a trigger.
#[must_use]
pub fn trigger_range(text: &str, caret: usize, open: &str, close: &str) -> Option<Range<usize>> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let start = text[..caret].rfind(open)?;
    let query = &text[start + open.len()..caret];
    if query.contains('\n') || query.contains(close) {
        return None;
    }
    let end = if text[caret..].starts_with(close) {
        caret + close.len()
    } else {
        caret
    };
    Some(start..end)
}

fn needs_brackets(title: &str) -> bool {
    title
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | ',' | '#' | '(' | ')'))
}

/// Insert rules for a page choice: `[[Title]]` after `[[`; `#tag` for a hashtag, `#[[multi word]]`
/// when the title has white space or special characters. `range` comes from [`trigger_range`] (or,
/// for `#`, from the `#` to the caret).
#[must_use]
pub fn complete_page(
    text: &str,
    range: Range<usize>,
    title: &str,
    hashtag: bool,
    exists: bool,
) -> Completion {
    let (new, trailing) = if hashtag {
        let body = if needs_brackets(title) {
            format!("#[[{title}]]")
        } else {
            format!("#{title}")
        };
        let space = !text[range.end.min(text.len())..].starts_with(char::is_whitespace);
        (body, space)
    } else {
        (format!("[[{title}]]"), false)
    };
    let mut ins = new;
    if trailing {
        ins.push(' ');
    }
    let caret = range.start + ins.len();
    Completion {
        range,
        text: ins,
        caret,
        create_page: (!exists).then(|| title.to_owned()),
    }
}

/// `((uuid))`.
#[must_use]
pub fn block_ref_text(uuid: Uuid) -> String {
    format!("(({}))", uuid.hyphenated())
}

/// `{{embed ((uuid))}}`.
#[must_use]
pub fn block_embed_text(uuid: Uuid) -> String {
    format!("{{{{embed (({}))}}}}", uuid.hyphenated())
}

fn fresh_uuid(ws: &Workspace) -> Uuid {
    loop {
        let u = Uuid::new_v4();
        if ws.uuid_count(&u) == 0 {
            return u;
        }
    }
}

/// `id:: <uuid>` after the title line; unchanged (refused as no-op) when the block has one.
pub(super) fn ensure_uuid(
    ws: &Workspace,
    id: BlockId,
    uuid: Option<Uuid>,
) -> Result<Vec<Op>, Refusal> {
    let b = ws.block(id).ok_or(Refusal::UnknownBlock(id))?;
    if let Some(page) = ws.locate(id) {
        check_writable(ws, page)?;
    }
    if b.uuid.is_some() {
        return Err(Refusal::NoChange);
    }
    let u = uuid.unwrap_or_else(|| fresh_uuid(ws));
    let (text, _) = ensure_block_id(&b.text, u, false).map_err(|_| Refusal::NothingApplicable)?;
    Ok(vec![set_text(ws, id, text)?])
}

/// Alt-drop: ensures every source has an `id::` and adds one `((uuid))` block each at `target`.
pub(super) fn drop_block_ref(
    ws: &Workspace,
    sources: &[BlockId],
    target: Target,
) -> Result<Planned, Refusal> {
    if sources.is_empty() {
        return Err(Refusal::EmptySelection);
    }
    let anchor = match target {
        Target::Before(t) | Target::After(t) | Target::FirstChild(t) | Target::LastChild(t) => t,
    };
    let apos = ws
        .position_of(anchor)
        .ok_or(Refusal::UnknownBlock(anchor))?;
    check_writable(ws, &apos.page)?;
    let (parent, index) = match target {
        Target::Before(_) => (apos.parent, apos.index),
        Target::After(_) => (apos.parent, apos.index + 1),
        Target::FirstChild(_) => (Some(anchor), 0),
        Target::LastChild(_) => (
            Some(anchor),
            ws.block(anchor).map_or(0, |b| b.children.len()),
        ),
    };
    let mut ops = Vec::new();
    let mut refs = Vec::new();
    for source in sources {
        let src = ws.block(*source).ok_or(Refusal::UnknownBlock(*source))?;
        let uuid = if let Some(u) = src.uuid {
            u
        } else {
            let u = fresh_uuid(ws);
            ops.extend(ensure_uuid(ws, *source, Some(u))?);
            u
        };
        refs.push(block_ref_text(uuid));
    }
    for (k, text) in refs.into_iter().enumerate() {
        ops.push(Op::InsertSubtree {
            page: apos.page.clone(),
            parent,
            index: index + k,
            subtree: Subtree::new(ws.alloc_id(), text),
        });
    }
    Ok(Planned {
        ops,
        cursor_after: None,
    })
}

pub(super) fn insert_block_ref(
    ws: &Workspace,
    target: BlockId,
    range: &Range<usize>,
    referenced: BlockId,
) -> Result<Planned, Refusal> {
    if target == referenced {
        return Err(Refusal::NothingApplicable);
    }
    let r = ws
        .block(referenced)
        .ok_or(Refusal::UnknownBlock(referenced))?;
    let mut ops = Vec::new();
    let uuid = if let Some(u) = r.uuid {
        u
    } else {
        let u = fresh_uuid(ws);
        ops.extend(ensure_uuid(ws, referenced, Some(u))?);
        u
    };
    let ins = block_ref_text(uuid);
    let edit = super::split::edit_text(ws, target, range, &ins)?;
    ops.extend(edit.ops);
    Ok(Planned {
        ops,
        cursor_after: Some(CursorState {
            block: target,
            selection: range.start + ins.len()..range.start + ins.len(),
        }),
    })
}

impl Workspace {
    /// Mod+C / Mod+E on a block: makes sure it has an `id::` (one transaction, none when it
    /// already has one) and returns the text to copy.
    ///
    /// # Errors
    /// [`CommitError`] when the block is unknown or its page read-only.
    pub fn copy_block_ref(
        &mut self,
        label: &'static str,
        id: BlockId,
        embed: bool,
    ) -> Result<(String, Option<Transaction>), CommitError> {
        let tx = match self.run(label, &Cmd::EnsureUuid { id, uuid: None }) {
            Ok(tx) => Some(tx),
            Err(CommitError::Refused(Refusal::NoChange)) => None,
            Err(e) => return Err(e),
        };
        let uuid = self
            .block(id)
            .and_then(|b| b.uuid)
            .ok_or(CommitError::Refused(Refusal::UnknownBlock(id)))?;
        let text = if embed {
            block_embed_text(uuid)
        } else {
            block_ref_text(uuid)
        };
        Ok((text, tx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_ranges() {
        let t = "see [[Pa]] now";
        assert_eq!(trigger_range(t, 8, "[[", "]]"), Some(4..10));
        assert_eq!(trigger_range("[[a]] b", 7, "[[", "]]"), None);
        assert_eq!(trigger_range("x ((q", 5, "((", "))"), Some(2..5));
    }

    #[test]
    fn page_insert_rules() {
        let t = "see [[Pa]] now";
        let c = complete_page(t, 4..10, "Page One", false, true);
        assert_eq!(c.text, "[[Page One]]");
        assert_eq!(c.caret, 16);
        assert_eq!(c.create_page, None);
        let tag = complete_page("x #foo", 2..6, "multi word", true, false);
        assert_eq!(tag.text, "#[[multi word]] ");
        assert_eq!(tag.create_page.as_deref(), Some("multi word"));
        assert_eq!(complete_page("#a", 0..2, "tag", true, true).text, "#tag ");
    }

    #[test]
    fn ref_texts() {
        let u = Uuid::nil();
        assert_eq!(
            block_ref_text(u),
            "((00000000-0000-0000-0000-000000000000))"
        );
        assert!(block_embed_text(u).starts_with("{{embed (("));
    }
}
