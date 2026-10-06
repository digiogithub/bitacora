//! Text helpers shared by the command planners: property groups, collapsed state, merging.

use bitacora_markdown::edit::properties::{get_property, set_property};
use bitacora_markdown::lines::ParserOptions;
use bitacora_markdown::properties::{GroupOrigin, PropertyGroup, scan_properties};
use bitacora_markdown::span::Span;

use super::model::{Block, BlockId};
use super::workspace::Workspace;

/// Property keys Logseq hides in the editor and keeps with the block that owns the identity: they
/// stay on the left block of a split.
const HIDDEN_KEYS: [&str; 3] = ["id", "custom-id", "collapsed"];

fn effective_group(text: &str) -> Option<PropertyGroup> {
    scan_properties(
        text.as_bytes(),
        Span::new(0, text.len()),
        ParserOptions::default(),
    )
    .groups
    .into_iter()
    .next()
}

/// True when the block carries `collapsed:: true`.
#[must_use]
pub fn is_collapsed(text: &str) -> bool {
    get_property(text, "collapsed").is_some_and(|v| v.trim() == "true")
}

/// True when the children of `b` are shown (it has children and is not collapsed).
#[must_use]
pub fn has_visible_children(b: &Block) -> bool {
    !b.children.is_empty() && !is_collapsed(&b.text)
}

/// True for text that is empty or only white space.
#[must_use]
pub fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Splits a block text into `(body, properties)` where `properties` is the effective inline
/// property group when it is the last thing in the text. The separating line break belongs to
/// neither part; `body.len()` is where appended text goes.
pub(crate) fn body_end(text: &str) -> usize {
    let Some(g) = effective_group(text) else {
        return text.len();
    };
    if g.origin != GroupOrigin::Inline || g.span.end < text.trim_end().len() {
        return text.len();
    }
    let start = g.span.start;
    if start > 0 && text.as_bytes()[start - 1] == b'\n' {
        start - 1
    } else {
        start
    }
}

/// The text without its property lines (any origin) and without trailing white space.
#[must_use]
pub fn strip_properties(text: &str) -> String {
    let Some(g) = effective_group(text) else {
        return text.trim_end().to_owned();
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..g.span.start]);
    out.push_str(&text[g.span.end.min(text.len())..]);
    out.trim_end().to_owned()
}

/// `(key, value)` of every valid `key:: value` line of the effective group except `skip`.
pub(crate) fn property_lines(text: &str) -> Vec<(String, String)> {
    effective_group(text)
        .map(|g| {
            g.lines
                .iter()
                .filter(|l| l.valid)
                .map(|l| (l.key_raw.clone(), l.value_raw.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Moves the hidden property lines (`id::`, `collapsed::`) out of `tail` and returns the tail
/// without them plus the lines (as `(key, value)`), preserving order.
pub(crate) fn take_hidden(tail: &str) -> (String, Vec<(String, String)>) {
    let Some(g) = effective_group(tail) else {
        return (tail.to_owned(), Vec::new());
    };
    if g.origin != GroupOrigin::Inline {
        return (tail.to_owned(), Vec::new());
    }
    let mut taken = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    for l in &g.lines {
        if l.valid && HIDDEN_KEYS.contains(&l.key_norm.as_str()) {
            taken.push((l.key_raw.clone(), l.value_raw.clone()));
            spans.push(l.span);
        }
    }
    if taken.is_empty() {
        return (tail.to_owned(), taken);
    }
    let mut out = String::with_capacity(tail.len());
    let mut at = 0;
    for s in &spans {
        out.push_str(&tail[at..s.start]);
        at = s.end.min(tail.len());
    }
    out.push_str(&tail[at..]);
    (out.trim_end().to_owned(), taken)
}

/// Adds `lines` to `text` as properties unless the key already exists.
pub(crate) fn add_properties(text: &str, lines: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for (k, v) in lines {
        if get_property(&out, k).is_none() {
            out = set_property(&out, k, v);
        }
    }
    out
}

/// Appends `addition` to the visible body of `text` (before a trailing property group).
/// Returns the new text and the byte offset of the junction.
pub(crate) fn append_to_body(text: &str, addition: &str) -> (String, usize) {
    let at = body_end(text);
    let mut out = String::with_capacity(text.len() + addition.len());
    out.push_str(&text[..at]);
    out.push_str(addition);
    out.push_str(&text[at..]);
    (out, at)
}

/// The previous block in reading order: the deepest visible last descendant of the previous
/// sibling, or the parent for a first child.
pub(crate) fn prev_visible(ws: &Workspace, id: BlockId) -> Option<BlockId> {
    let pos = ws.position_of(id)?;
    let page = ws.page(&pos.page)?;
    if pos.index == 0 {
        return pos.parent;
    }
    let sibs = page.children_of(pos.parent)?;
    let mut cur = sibs[pos.index - 1];
    loop {
        let b = ws.block(cur)?;
        if !has_visible_children(b) {
            return Some(cur);
        }
        cur = *b.children.last()?;
    }
}

/// Logseq's "left" of a block: its previous sibling, or the parent for a first child.
pub(crate) fn left_of(ws: &Workspace, id: BlockId) -> Option<BlockId> {
    let pos = ws.position_of(id)?;
    if pos.index == 0 {
        return pos.parent;
    }
    ws.page(&pos.page)?
        .children_of(pos.parent)?
        .get(pos.index - 1)
        .copied()
}

/// Next sibling of a block.
pub(crate) fn right_of(ws: &Workspace, id: BlockId) -> Option<BlockId> {
    let pos = ws.position_of(id)?;
    ws.page(&pos.page)?
        .children_of(pos.parent)?
        .get(pos.index + 1)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_and_body_end() {
        assert!(is_collapsed("a\ncollapsed:: true"));
        assert!(!is_collapsed("a\ncollapsed:: false"));
        let t = "title\nid:: 6f2c1b7a-0000-4000-8000-000000000001";
        assert_eq!(body_end(t), 5);
        assert_eq!(
            append_to_body(t, " more").0,
            format!("title more{}", &t[5..])
        );
        assert_eq!(body_end("plain"), 5);
    }

    #[test]
    fn hidden_lines_leave_the_tail() {
        let (tail, hidden) = take_hidden("rest\ncollapsed:: true\nfoo:: bar");
        assert_eq!(tail, "rest\nfoo:: bar");
        assert_eq!(hidden, vec![("collapsed".to_owned(), "true".to_owned())]);
    }

    #[test]
    fn strip_properties_drops_the_group() {
        assert_eq!(strip_properties("x\na:: b\nc:: d"), "x");
    }
}
