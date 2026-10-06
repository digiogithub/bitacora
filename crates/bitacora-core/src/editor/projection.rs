//! The edit projection: what the editor shows for a block (BIT-US-0030, BIT-T-0137;
//! `docs/design/block-editor.md` §2.3).
//!
//! Logseq hides the built-in properties (`id::`, `collapsed::`, ...) and the `:LOGBOOK:` drawer
//! while a block is edited and re-injects them on save. We do the same, but each hidden line
//! remembers how many visible lines precede it and goes back to exactly that place, so the lines
//! of an untouched block come back byte for byte and the hidden metadata never moves behind the
//! body text (where Logseq would stop recognising it).
//!
//! Offsets are UTF-8 byte offsets on char boundaries. The text is the full block text as the
//! core stores it (see [`super::model::Block::text`]).

use std::collections::BTreeSet;

use bitacora_markdown::lines::ParserOptions;
use bitacora_markdown::properties::{GroupOrigin, scan_properties};
use bitacora_markdown::span::Span;

/// Which property keys are hidden in the editor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HiddenKeys {
    /// Extra normalised keys (`:block-hidden-properties` of `config.edn`).
    pub extra: BTreeSet<String>,
}

impl HiddenKeys {
    /// The built-in set plus `extra` (keys are normalised: lower case, `_` and space as `-`).
    #[must_use]
    pub fn with_extra(extra: impl IntoIterator<Item = String>) -> Self {
        Self {
            extra: extra
                .into_iter()
                .map(|k| bitacora_markdown::properties::normalize_key(&k))
                .collect(),
        }
    }

    /// Whether the property `key_norm` (normalised) with `value` is hidden.
    #[must_use]
    pub fn is_hidden(&self, key_norm: &str, value: &str) -> bool {
        const BUILT_IN: [&str; 11] = [
            "id",
            "collapsed",
            "background-color",
            "created-at",
            "updated-at",
            "last-modified-at",
            "query-table",
            "query-properties",
            "query-sort-by",
            "query-sort-desc",
            "ls-type",
        ];
        BUILT_IN.contains(&key_norm)
            || (key_norm == "heading" && matches!(value.trim(), "true" | "false"))
            || key_norm.starts_with("hl-")
            || key_norm.starts_with("logseq.macro-")
            || key_norm.starts_with("logseq.order-list-type")
            || self.extra.contains(key_norm)
    }
}

/// A line taken out of the visible text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hidden {
    /// The line without its line break.
    line: String,
    /// How many visible lines precede it.
    anchor: usize,
}

/// A block text split into what the editor shows and the lines it keeps aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditProjection {
    visible: String,
    hidden: Vec<Hidden>,
    /// Number of visible lines of the original text (0 for an all-hidden text).
    visible_lines: usize,
    /// The full text this projection was made from.
    original: String,
}

/// Lines of `text` split at `\n` (a `\r` stays at the end of its line, so CRLF survives).
fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

impl EditProjection {
    /// Projects `text`.
    #[must_use]
    pub fn from_text(text: &str, keys: &HiddenKeys) -> Self {
        let lines = lines_of(text);
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for l in &lines {
            starts.push(at);
            at += l.len() + 1;
        }
        let line_of = |offset: usize| starts.partition_point(|s| *s <= offset).saturating_sub(1);
        let mut is_hidden = vec![false; lines.len()];

        let scan = scan_properties(
            text.as_bytes(),
            Span::new(0, text.len()),
            ParserOptions::default(),
        );
        if let Some(group) = scan.groups.first()
            && group.origin == GroupOrigin::Inline
        {
            for l in &group.lines {
                if l.valid && keys.is_hidden(&l.key_norm, &l.value_raw) {
                    let ix = line_of(l.span.start);
                    // A property right after the bullet shares its line with nothing else.
                    if lines[ix].trim_start().starts_with(l.key_raw.as_str()) {
                        is_hidden[ix] = true;
                    }
                }
            }
        }
        // The LOGBOOK drawer, including both fences.
        let mut in_drawer = false;
        for (ix, l) in lines.iter().enumerate() {
            let t = l.trim();
            if !in_drawer && t.eq_ignore_ascii_case(":LOGBOOK:") {
                in_drawer = true;
            }
            if in_drawer {
                is_hidden[ix] = true;
                if t.eq_ignore_ascii_case(":END:") {
                    in_drawer = false;
                }
            }
        }

        let mut visible = Vec::new();
        let mut hidden = Vec::new();
        for (ix, l) in lines.iter().enumerate() {
            if is_hidden[ix] {
                hidden.push(Hidden {
                    line: (*l).to_owned(),
                    anchor: visible.len(),
                });
            } else {
                visible.push(*l);
            }
        }
        // A text made only of hidden lines has no visible line, not one empty line.
        let visible_lines = visible.len();
        Self {
            visible: visible.join("\n"),
            hidden,
            visible_lines,
            original: text.to_owned(),
        }
    }

    /// The text to show in the editor.
    #[must_use]
    pub fn visible(&self) -> &str {
        &self.visible
    }

    /// True when some line is hidden.
    #[must_use]
    pub fn has_hidden(&self) -> bool {
        !self.hidden.is_empty()
    }

    /// The full text this projection was built from.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Lines of `edited` as they take part in the merge: `""` is zero lines when the original had
    /// none, otherwise one empty line.
    fn edited_lines<'a>(&self, edited: &'a str) -> Vec<&'a str> {
        if edited.is_empty() && self.visible_lines == 0 {
            Vec::new()
        } else {
            lines_of(edited)
        }
    }

    /// The full text after the user edited the visible text to `edited`: hidden lines return to
    /// the place where they were (clamped to the end).
    #[must_use]
    pub fn to_text(&self, edited: &str) -> String {
        let lines = self.edited_lines(edited);
        let mut out: Vec<&str> = Vec::with_capacity(lines.len() + self.hidden.len());
        let mut next = 0;
        for (ix, l) in lines.iter().enumerate() {
            while next < self.hidden.len() && self.hidden[next].anchor.min(lines.len()) <= ix {
                out.push(&self.hidden[next].line);
                next += 1;
            }
            out.push(l);
        }
        for h in &self.hidden[next..] {
            out.push(&h.line);
        }
        out.join("\n")
    }

    /// Maps a byte offset of `edited` to the matching offset of [`Self::to_text`]`(edited)`.
    #[must_use]
    pub fn visible_to_full(&self, edited: &str, offset: usize) -> usize {
        let offset = offset.min(edited.len());
        let lines = self.edited_lines(edited);
        // Line holding the offset and the offset of its start.
        let mut start = 0;
        let mut line_ix = 0;
        for (ix, l) in lines.iter().enumerate() {
            line_ix = ix;
            if offset <= start + l.len() {
                break;
            }
            start += l.len() + 1;
        }
        let before: usize = self
            .hidden
            .iter()
            .filter(|h| h.anchor.min(lines.len()) <= line_ix)
            .map(|h| h.line.len() + 1)
            .sum();
        if lines.is_empty() {
            // Only hidden lines: the caret goes before them.
            return 0;
        }
        offset + before
    }

    /// Maps a byte offset of the full text `full` (as [`Self::original`] of a projection made
    /// from it) to the visible text. An offset inside a hidden line lands at the end of the
    /// visible line before it.
    #[must_use]
    pub fn full_to_visible(&self, offset: usize) -> usize {
        let lines = lines_of(&self.original);
        let mut hidden_ix = 0;
        let mut visible_seen = 0;
        let mut full_at = 0;
        let mut vis_at: usize = 0;
        for l in &lines {
            let hidden_here = hidden_ix < self.hidden.len()
                && self.hidden[hidden_ix].anchor == visible_seen
                && self.hidden[hidden_ix].line == *l;
            let end = full_at + l.len();
            if hidden_here {
                hidden_ix += 1;
                if offset <= end {
                    return vis_at.saturating_sub(usize::from(visible_seen > 0));
                }
            } else {
                if offset <= end {
                    return vis_at + (offset - full_at).min(l.len());
                }
                vis_at += l.len() + 1;
                visible_seen += 1;
            }
            full_at = end + 1;
        }
        self.visible.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "id:: 6f2c1b7a-0000-4000-8000-000000000001";

    fn proj(text: &str) -> EditProjection {
        EditProjection::from_text(text, &HiddenKeys::default())
    }

    #[test]
    fn id_and_collapsed_are_hidden_and_come_back_unchanged() {
        let text = format!("title\n{ID}\ncollapsed:: true\nfoo:: bar");
        let p = proj(&text);
        assert_eq!(p.visible(), "title\nfoo:: bar");
        assert!(p.has_hidden());
        assert_eq!(p.to_text(p.visible()), text);
    }

    #[test]
    fn untouched_text_round_trips_for_odd_shapes() {
        for text in [
            "",
            "plain",
            "a\nb\nc",
            "a\n",
            "\n",
            "\nid:: x1",
            "id:: abc",
            "a\r\nb\r\nid:: x",
            "TODO a\nSCHEDULED: <2024-01-01 Mon>\nid:: u",
            "t\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\n:END:",
            "t\nid:: u\n:LOGBOOK:\nCLOCK: x\n:END:\nbody",
            "t\n  id:: u",
        ] {
            let p = proj(text);
            assert_eq!(p.to_text(p.visible()), text, "round trip of {text:?}");
        }
    }

    #[test]
    fn hidden_lines_stay_after_the_title_when_text_is_added() {
        let text = format!("title\n{ID}");
        let p = proj(&text);
        assert_eq!(p.visible(), "title");
        assert_eq!(p.to_text("title\nmore"), format!("title\n{ID}\nmore"));
        assert_eq!(p.to_text("new title"), format!("new title\n{ID}"));
        assert_eq!(p.to_text(""), format!("\n{ID}"));
    }

    #[test]
    fn hidden_line_in_the_middle_keeps_its_anchor() {
        let text = format!("a\n{ID}\nbody");
        let p = proj(&text);
        assert_eq!(p.visible(), "a\nbody");
        assert_eq!(p.to_text("a\nbody\nmore"), format!("a\n{ID}\nbody\nmore"));
        assert_eq!(p.to_text("only"), format!("only\n{ID}"));
    }

    #[test]
    fn only_hidden_lines_means_no_visible_lines() {
        let p = proj("id:: u");
        assert_eq!(p.visible(), "");
        assert_eq!(p.to_text(""), "id:: u");
        assert_eq!(p.to_text("x"), "id:: u\nx");
    }

    #[test]
    fn logbook_is_hidden() {
        let text = "NOW work\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\n:END:";
        let p = proj(text);
        assert_eq!(p.visible(), "NOW work");
        assert_eq!(
            p.to_text("NOW working"),
            "NOW working\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\n:END:"
        );
    }

    #[test]
    fn heading_is_hidden_only_when_boolean_and_extra_keys_apply() {
        let p = proj("t\nheading:: true");
        assert_eq!(p.visible(), "t");
        let p = proj("t\nheading:: 2");
        assert_eq!(p.visible(), "t\nheading:: 2");
        let keys = HiddenKeys::with_extra(["Secret_Key".to_owned()]);
        let p = EditProjection::from_text("t\nsecret-key:: x\nother:: y", &keys);
        assert_eq!(p.visible(), "t\nother:: y");
    }

    #[test]
    fn a_trailing_property_group_after_the_body_is_hidden_and_restored_in_place() {
        let text = "t\nbody\nid:: late";
        let p = proj(text);
        assert_eq!(p.visible(), "t\nbody");
        assert_eq!(p.to_text("t\nbody"), text);
        assert_eq!(p.to_text("t\nbody\nmore"), "t\nbody\nid:: late\nmore");
    }

    #[test]
    fn offsets_map_in_both_directions() {
        let text = format!("title\n{ID}\nbody");
        let p = proj(&text);
        assert_eq!(p.visible(), "title\nbody");
        // End of "title" is before the hidden line; start of "body" is after it.
        assert_eq!(p.visible_to_full(p.visible(), 5), 5);
        assert_eq!(p.visible_to_full(p.visible(), 6), 6 + ID.len() + 1);
        assert_eq!(p.visible_to_full(p.visible(), 10), 10 + ID.len() + 1);
        // Full offsets: inside the hidden line they snap to the end of the title.
        assert_eq!(p.full_to_visible(3), 3);
        assert_eq!(p.full_to_visible(8), 5);
        assert_eq!(p.full_to_visible(6 + ID.len() + 1 + 2), 6 + 2);
        // The empty visible text still maps.
        let q = proj("id:: u");
        assert_eq!(q.visible_to_full("", 0), 0);
        assert_eq!(q.full_to_visible(2), 0);
    }

    #[test]
    fn offset_round_trip_over_every_boundary() {
        let text = format!("héllo\n{ID}\nsecond line\ncollapsed:: true");
        let p = proj(&text);
        let v = p.visible().to_owned();
        for (i, _) in v.char_indices().chain([(v.len(), ' ')]) {
            let full = p.visible_to_full(&v, i);
            assert!(text.is_char_boundary(full));
            assert_eq!(p.full_to_visible(full), i, "offset {i} via {full}");
        }
    }
}
