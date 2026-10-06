//! Span-based reference rewriting for page renames (BIT-US-0082; `docs/analysis/logseq/01-file-graph-layout.md`
//! section 10.2).
//!
//! [`rewrite_refs`] rewrites the references to a page inside one block's text. It never
//! re-serializes anything: it computes byte-range edits from the inline tokenizer and the property
//! scanner of `bitacora-markdown` and applies only those, so every byte that is not part of a
//! reference stays as it was.
//!
//! What is rewritten (`old` -> `new`):
//! * `[[Old]]`, also nested (`[[a [[Old]] b]]`), as the target of `[label]([[Old]])` and inside
//!   `{{embed [[Old]]}}`;
//! * `#Old` and `#[[Old]]`; a bare tag whose new name is not tag-safe (whitespace, punctuation)
//!   becomes `#[[New Name]]`;
//! * namespace references: `[[old/child]]` -> `[[new/child]]` (the prefix is replaced once);
//! * property keys: `old:: v` -> `new-name:: v` (lower-case, spaces become `-`; `id::` is never
//!   touched);
//! * property values: page references like in content, and, for the comma-separated keys
//!   (`alias`, `aliases`, `tags` and the configured ones), the plain fragments (`tags:: Old, x`).
//!
//! Not rewritten: inline code, math, fences, `#+BEGIN_SRC`-like regions, drawers, quoted property
//! values and the unparsed built-in keys (`title`, `id`, ...).
//!
//! Decision on Logseq's open question 2 (BIT-SP-0002): matching is **case-insensitive**, like page
//! identity itself. Logseq's own `replace-page-ref!` is case-sensitive and leaves `[[old]]`
//! dangling after `Old` is renamed; the result stays readable by Logseq.

use std::ops::Range;

use bitacora_markdown::ParserOptions;
use bitacora_markdown::block::inline_text_ranges;
use bitacora_markdown::inline::{InlineToken, LinkTarget, PageRef, scan_line};
use bitacora_markdown::properties::{PropValue, PropertyConfig, interpret, scan_properties};
use bitacora_markdown::span::Span;

/// One replacement of a byte range.
type Edit = (Range<usize>, String);

/// Compares `name` with the page `old` ignoring case: returns the byte length of the leading part
/// of `name` that equals `old` when that part is the whole name or is followed by `/`
/// (a namespace child).
fn match_len(name: &str, old: &str, children: bool) -> Option<usize> {
    let mut n = name.char_indices();
    for oc in old.chars() {
        let (_, nc) = n.next()?;
        if !nc.to_lowercase().eq(oc.to_lowercase()) {
            return None;
        }
    }
    let end = n.next().map_or(name.len(), |(i, _)| i);
    (end == name.len() || (children && name[end..].starts_with('/'))).then_some(end)
}

/// The renamed form of `name` (`new` plus the unmatched namespace tail), `None` when `name` does
/// not denote `old` or one of its namespace children.
pub(crate) fn renamed(name: &str, old: &str, new: &str, children: bool) -> Option<String> {
    let trimmed = name.trim();
    let len = match_len(trimmed, old.trim(), children)?;
    let tail = &trimmed[len..];
    Some(format!("{}{tail}", new.trim()))
}

/// True when `name` needs `#[[...]]` to be written as a tag.
fn needs_brackets(name: &str) -> bool {
    !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
}

struct Rewriter<'a> {
    text: &'a str,
    old: &'a str,
    new: &'a str,
    children: bool,
    edits: Vec<Edit>,
}

impl Rewriter<'_> {
    fn name_edit(&mut self, name: Span) {
        let raw = &self.text[name.range()];
        let Some(replacement) = renamed(raw, self.old, self.new, self.children) else {
            return;
        };
        let lead = raw.len() - raw.trim_start().len();
        let end = name.start + lead + raw.trim().len();
        self.edits.push((name.start + lead..end, replacement));
    }

    fn page_ref(&mut self, p: &PageRef) {
        if renamed(
            &self.text[p.name.range()],
            self.old,
            self.new,
            self.children,
        )
        .is_some()
        {
            self.name_edit(p.name);
        } else {
            for n in &p.nested {
                self.page_ref(n);
            }
        }
    }

    fn tokens(&mut self, tokens: &[InlineToken]) {
        for t in tokens {
            match t {
                InlineToken::PageRef(p) => self.page_ref(p),
                InlineToken::Tag(tag) => {
                    if let Some(new_name) = renamed(
                        &self.text[tag.name.range()],
                        self.old,
                        self.new,
                        self.children,
                    ) {
                        if tag.bracketed || !needs_brackets(&new_name) {
                            self.name_edit(tag.name);
                        } else {
                            self.edits
                                .push((tag.span.range(), format!("#[[{new_name}]]")));
                        }
                    } else {
                        for n in &tag.nested {
                            self.page_ref(n);
                        }
                    }
                }
                InlineToken::Link(l) => {
                    if let LinkTarget::Page(p) = &l.target {
                        self.page_ref(p);
                    }
                }
                InlineToken::Macro(m)
                    if self.text[m.name.range()].eq_ignore_ascii_case("embed") =>
                {
                    for a in &m.args {
                        let inner = scan_line(self.text, a.start, a.end);
                        self.tokens(&inner);
                    }
                }
                _ => {}
            }
        }
    }

    /// Plain fragments of a comma-separated value (the gaps between inline tokens).
    fn plain_fragments(&mut self, value: Span, tokens: &[InlineToken]) {
        let mut gaps: Vec<(usize, usize)> = Vec::new();
        let mut at = value.start;
        for t in tokens {
            let s = t.span();
            if s.start > at {
                gaps.push((at, s.start));
            }
            at = at.max(s.end);
        }
        if at < value.end {
            gaps.push((at, value.end));
        }
        for (lo, hi) in gaps {
            let mut frag_start = lo;
            let gap = &self.text[lo..hi];
            let mut cuts: Vec<(usize, usize)> = Vec::new();
            for (i, c) in gap.char_indices() {
                if c == ',' || c == '，' {
                    cuts.push((frag_start, lo + i));
                    frag_start = lo + i + c.len_utf8();
                }
            }
            cuts.push((frag_start, hi));
            for (a, b) in cuts {
                let frag = &self.text[a..b];
                let t = frag.trim();
                if t.is_empty() {
                    continue;
                }
                if let Some(mut r) = renamed(t, self.old, self.new, self.children) {
                    if r.contains([',', '，', '[', ']', '#']) {
                        r = format!("[[{r}]]");
                    }
                    let lead = frag.len() - frag.trim_start().len();
                    self.edits.push((a + lead..a + lead + t.len(), r));
                }
            }
        }
    }
}

fn comma_separated(key_norm: &str, cfg: &PropertyConfig) -> bool {
    matches!(key_norm, "alias" | "aliases" | "tags") || cfg.separated_by_commas.contains(key_norm)
}

/// The property key Logseq derives from a page name: lower-case, spaces become `-`.
fn key_for(title: &str) -> String {
    title.trim().to_lowercase().replace(' ', "-")
}

/// Rewrites the references to page `old` in a block's text (see the module docs). With `children`
/// the references to its namespace children (`[[old/x]]`) are rewritten too. Returns the new
/// text, or `None` when nothing in it refers to `old`.
#[must_use]
pub fn rewrite_refs(
    text: &str,
    old: &str,
    new: &str,
    cfg: &PropertyConfig,
    children: bool,
) -> Option<String> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return None;
    }
    let opts = ParserOptions::default();
    let mut rw = Rewriter {
        text,
        old,
        new,
        children,
        edits: Vec::new(),
    };
    for (lo, hi) in inline_text_ranges(text, opts) {
        let tokens = scan_line(text, lo, hi);
        rw.tokens(&tokens);
    }
    let scan = scan_properties(text.as_bytes(), Span::new(0, text.len()), opts);
    let old_key = key_for(old);
    for line in scan.valid_lines() {
        if line.key_norm == "id" {
            continue;
        }
        if line.key_norm == old_key {
            rw.edits.push((line.key_span.range(), key_for(new)));
        }
        match interpret(&line.key_norm, &line.value_raw, cfg) {
            PropValue::Raw(_) | PropValue::Quoted(_) => continue,
            _ => {}
        }
        let vs = line.value_span;
        if vs.is_empty() {
            continue;
        }
        let tokens = scan_line(text, vs.start, vs.end);
        rw.tokens(&tokens);
        if comma_separated(&line.key_norm, cfg) {
            rw.plain_fragments(vs, &tokens);
        }
    }
    if rw.edits.is_empty() {
        return None;
    }
    let mut edits = rw.edits;
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = String::with_capacity(text.len() + 16);
    let mut at = 0;
    for (r, s) in edits {
        if r.start < at {
            continue; // overlapping edit: the outer one wins
        }
        out.push_str(&text[at..r.start]);
        out.push_str(&s);
        at = r.end;
    }
    out.push_str(&text[at..]);
    (out != text).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rw(text: &str, old: &str, new: &str) -> Option<String> {
        rewrite_refs(text, old, new, &PropertyConfig::default(), true)
    }

    #[test]
    fn page_refs_and_tags() {
        assert_eq!(
            rw("see [[Old]] and #Old", "Old", "New Idea").as_deref(),
            Some("see [[New Idea]] and #[[New Idea]]")
        );
        assert_eq!(
            rw("#[[Old]] x", "Old", "New Idea").as_deref(),
            Some("#[[New Idea]] x")
        );
        assert_eq!(rw("#Old", "Old", "new").as_deref(), Some("#new"));
        assert_eq!(rw("nothing here", "Old", "New"), None);
        assert_eq!(rw("[[Older]] #Older", "Old", "New"), None);
    }

    #[test]
    fn matching_is_case_insensitive_and_trimmed() {
        assert_eq!(
            rw("[[old]] [[ OLD ]]", "Old", "New").as_deref(),
            Some("[[New]] [[ New ]]")
        );
    }

    #[test]
    fn namespace_children_and_nested() {
        assert_eq!(
            rw("[[a/x]] #a/y [[a]]", "a", "b").as_deref(),
            Some("[[b/x]] #b/y [[b]]")
        );
        assert_eq!(
            rw("[[outer [[Old]] more]]", "Old", "New").as_deref(),
            Some("[[outer [[New]] more]]")
        );
        assert_eq!(rw("[[ab/x]]", "a", "b"), None);
    }

    #[test]
    fn namespace_children_can_be_left_alone() {
        let cfg = PropertyConfig::default();
        assert_eq!(
            rewrite_refs("[[a/x]] [[a]]", "a", "b", &cfg, false).as_deref(),
            Some("[[a/x]] [[b]]")
        );
    }

    #[test]
    fn link_targets_and_embeds() {
        assert_eq!(
            rw("[label]([[Old]]) {{embed [[Old]]}}", "Old", "N").as_deref(),
            Some("[label]([[N]]) {{embed [[N]]}}")
        );
    }

    #[test]
    fn code_and_fences_are_untouched() {
        let t = "x `[[Old]]` y\n```\n[[Old]] #Old\n```\n[[Old]]";
        assert_eq!(
            rw(t, "Old", "New").as_deref(),
            Some("x `[[Old]]` y\n```\n[[Old]] #Old\n```\n[[New]]")
        );
        assert_eq!(rw("`[[Old]]`", "Old", "New"), None);
    }

    #[test]
    fn property_keys_and_values() {
        assert_eq!(
            rw("- x\n  old:: value", "Old", "New Idea").as_deref(),
            Some("- x\n  new-idea:: value")
        );
        assert_eq!(
            rw("tags:: Old, x", "Old", "New Idea").as_deref(),
            Some("tags:: New Idea, x")
        );
        assert_eq!(
            rw("tags:: [[Old]], #Old, y", "Old", "New Idea").as_deref(),
            Some("tags:: [[New Idea]], #[[New Idea]], y")
        );
        assert_eq!(
            rw("alias:: a, Old", "Old", "B, C").as_deref(),
            Some("alias:: a, [[B, C]]")
        );
        assert_eq!(rw("foo:: Old", "Old", "New"), None);
        assert_eq!(rw("title:: Old", "Old", "New"), None);
        assert_eq!(rw("tags:: \"Old\"", "Old", "New"), None);
        assert_eq!(
            rw("id:: 6500c1a4-0000-4000-8000-000000000001", "id", "x"),
            None
        );
    }

    #[test]
    fn untouched_bytes_are_preserved() {
        let t = "TODO [#A]   see  [[Old]]\t#x\n  prop:: v\n  more   text";
        assert_eq!(
            rw(t, "Old", "N").as_deref(),
            Some("TODO [#A]   see  [[N]]\t#x\n  prop:: v\n  more   text")
        );
    }
}
