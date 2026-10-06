//! Property line and group scanner.
//!
//! Rules (observed on mldoc 1.5.7, documented in
//! `docs/analysis/logseq/02-markdown-block-syntax.md` §3.1):
//!
//! * A property line is `[ \t]* key "::" ( " " value | [ \t]* EOL )` where `key` is one or more
//!   bytes that are neither `:` nor a space or tab. `key::value` and `a:b:: v` are not properties.
//! * The text right after a block's bullet (`- a:: b`) may itself be a property line.
//! * Consecutive property lines form one group; `#+name: value` lines directly after a property
//!   line join the group. Any other line, including a blank one, ends it.
//! * Lines inside quotes (`> ...`), fences, `#+BEGIN_X` blocks and front matter are never
//!   properties.
//! * The first group of a block is the *effective* one.

use crate::lines::{Line, LineKind, Lines, ParserOptions, is_ws};
use crate::properties::drawer;
use crate::span::Span;

/// How a property line was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropLineKind {
    /// `key:: value`.
    Property,
    /// `#+key: value` joined to a property group.
    Directive,
    /// `:key: value` inside a `:PROPERTIES:` drawer.
    Drawer,
}

/// One property line with its original byte spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropLine {
    /// The whole line including its EOL (for a property right after a bullet: from the key start).
    pub span: Span,
    /// The key as written.
    pub key_span: Span,
    /// The trimmed value as written.
    pub value_span: Span,
    /// The key as written.
    pub key_raw: String,
    /// The key normalised for lookup (see [`normalize_key`]).
    pub key_norm: String,
    /// The trimmed value as written (`""` for `key::`).
    pub value_raw: String,
    /// False when Logseq would reject the key (`"x"::`, `(a)::`, `#a::`, ...). The line is kept.
    pub valid: bool,
    /// How the line was written.
    pub kind: PropLineKind,
}

/// Where a group comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupOrigin {
    /// Plain `key:: value` lines.
    Inline,
    /// A `:PROPERTIES:` ... `:END:` drawer (converted to `key:: value` only when the block is
    /// edited; never on read).
    Drawer,
}

/// A run of consecutive property lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyGroup {
    /// From the first line to the end of the last one (drawers: `:PROPERTIES:` to `:END:`).
    pub span: Span,
    /// The property lines in order.
    pub lines: Vec<PropLine>,
    /// Inline lines or a drawer.
    pub origin: GroupOrigin,
    /// True for the first group of the block (the one Logseq uses).
    pub effective: bool,
}

/// All property groups found in a block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropertyScan {
    /// Groups in document order.
    pub groups: Vec<PropertyGroup>,
}

impl PropertyScan {
    /// The effective (first) group.
    #[must_use]
    pub fn effective(&self) -> Option<&PropertyGroup> {
        self.groups.first()
    }

    /// Every property line of the effective group with a valid key.
    pub fn valid_lines(&self) -> impl Iterator<Item = &PropLine> {
        self.effective()
            .into_iter()
            .flat_map(|g| g.lines.iter())
            .filter(|l| l.valid)
    }

    /// Every property line of the whole block whose key Logseq rejects.
    pub fn invalid_lines(&self) -> impl Iterator<Item = &PropLine> {
        self.groups
            .iter()
            .flat_map(|g| g.lines.iter())
            .filter(|l| !l.valid)
    }
}

/// Normalises a key for lookup: lower-case, spaces and underscores become `-`, and
/// `custom-id` / `custom_id` become `id`.
#[must_use]
pub fn normalize_key(raw: &str) -> String {
    let k: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' || c == '_' { '-' } else { c })
        .collect();
    if k == "custom-id" { "id".to_owned() } else { k }
}

/// Whether Logseq accepts a *normalised* key as a property name: a valid EDN keyword name without
/// `" | ^ ( ) { }`, not starting with `#`. The EDN part is approximated: no brackets, commas,
/// semicolons, backslashes or whitespace, and the name may not start with a digit.
#[must_use]
pub fn is_valid_key(norm: &str) -> bool {
    let name = norm.rsplit('/').next().unwrap_or(norm);
    !norm.is_empty()
        && !name.is_empty()
        && !norm.starts_with('#')
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && !norm.chars().any(|c| {
            c.is_whitespace()
                || matches!(
                    c,
                    '"' | '|' | '^' | '(' | ')' | '{' | '}' | '[' | ']' | ',' | ';' | '\\'
                )
        })
}

pub(crate) fn make_line(
    input: &[u8],
    span: Span,
    key: Span,
    value: Span,
    kind: PropLineKind,
    norm: impl FnOnce(&str) -> String,
) -> PropLine {
    let key_raw = String::from_utf8_lossy(key.slice(input)).into_owned();
    let value_raw = String::from_utf8_lossy(value.slice(input)).into_owned();
    let key_norm = norm(&key_raw);
    let valid = is_valid_key(&key_norm);
    PropLine {
        span,
        key_span: key,
        value_span: value,
        key_raw,
        key_norm,
        value_raw,
        valid,
        kind,
    }
}

/// Offsets of `key` and `value` inside `text` for a `key:: value` line.
fn parse_property(text: &[u8]) -> Option<(Span, Span)> {
    let key_len = text.iter().take_while(|&&b| b != b':' && !is_ws(b)).count();
    if key_len == 0 || text.get(key_len) != Some(&b':') || text.get(key_len + 1) != Some(&b':') {
        return None;
    }
    let after = key_len + 2;
    let rest = &text[after..];
    let value = if rest.is_empty() {
        Span::new(after, after)
    } else if rest[0] == b' ' || rest.iter().all(|&b| is_ws(b)) {
        trim_ws(text, after, text.len())
    } else {
        return None;
    };
    Some((Span::new(0, key_len), value))
}

/// Offsets of `name` and `value` inside `text` for a `#+name: value` line.
fn parse_directive(text: &[u8]) -> Option<(Span, Span)> {
    let body = text.strip_prefix(b"#+")?;
    let name_len = body.iter().take_while(|&&b| b != b':' && !is_ws(b)).count();
    if name_len == 0 || body.get(name_len) != Some(&b':') {
        return None;
    }
    let after = 2 + name_len + 1;
    if text.get(after).is_some_and(|&b| !is_ws(b)) {
        return None;
    }
    Some((Span::new(2, 2 + name_len), trim_ws(text, after, text.len())))
}

fn trim_ws(text: &[u8], mut start: usize, mut end: usize) -> Span {
    while start < end && is_ws(text[start]) {
        start += 1;
    }
    while end > start && is_ws(text[end - 1]) {
        end -= 1;
    }
    Span::new(start, end)
}

fn shift(s: Span, by: usize) -> Span {
    Span::new(s.start + by, s.end + by)
}

const BOM: &[u8] = b"\xef\xbb\xbf";

/// Offset of the text that may hold a property: after the bullet, or after the indentation. A BOM
/// at the very start of the input is skipped so it never leaks into a key (BIT-SP-0001.R19).
fn text_start(line: &Line<'_>) -> usize {
    match line.kind {
        LineKind::BulletStart { after_dash, .. } => after_dash,
        _ if line.start == 0 && line.content.starts_with(BOM) => {
            let ws = line.content[BOM.len()..]
                .iter()
                .take_while(|&&b| is_ws(b))
                .count();
            BOM.len() + ws
        }
        _ => line.indent.len(),
    }
}

/// Scans the byte range `range` of `input` (a block or the pre-block) for property groups.
///
/// `range` should be a block span from [`crate::outline::split`]; regions (fences, `#+BEGIN_X`,
/// front matter) are re-detected inside it, so properties in them are ignored.
#[must_use]
pub fn scan_properties(input: &[u8], range: Span, opts: ParserOptions) -> PropertyScan {
    let slice = range.slice(input);
    let lines: Vec<Line<'_>> = Lines::with_options(slice, opts).collect();
    let base = range.start;
    let mut groups: Vec<PropertyGroup> = Vec::new();
    let mut current: Option<PropertyGroup> = None;
    let flush = |current: &mut Option<PropertyGroup>, groups: &mut Vec<PropertyGroup>| {
        if let Some(g) = current.take() {
            groups.push(g);
        }
    };

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        i += 1;
        if line.region.is_some() {
            flush(&mut current, &mut groups);
            continue;
        }
        let ts = text_start(line);
        let text = &line.content[ts.min(line.content.len())..];

        if let Some((group, next)) = drawer::read_drawer(input, base, &lines, i - 1) {
            flush(&mut current, &mut groups);
            groups.push(group);
            i = next;
            continue;
        }
        if line.content.get(line.indent.len()) == Some(&b'>') {
            flush(&mut current, &mut groups);
            continue;
        }

        // A property right after a bullet starts at the key; any other line is taken whole.
        let from = if matches!(line.kind, LineKind::BulletStart { .. }) {
            ts
        } else {
            0
        };
        let span_start = base + line.start + from;
        let line_span = Span::new(span_start, base + line.end);
        if let Some((key, value)) = parse_property(text) {
            let pl = make_line(
                input,
                line_span,
                shift(key, base + line.start + ts),
                shift(value, base + line.start + ts),
                PropLineKind::Property,
                normalize_key,
            );
            push_line(&mut current, pl);
        } else if current.is_some() {
            if let Some((key, value)) = parse_directive(text) {
                let pl = make_line(
                    input,
                    line_span,
                    shift(key, base + line.start + ts),
                    shift(value, base + line.start + ts),
                    PropLineKind::Directive,
                    normalize_key,
                );
                push_line(&mut current, pl);
            } else {
                flush(&mut current, &mut groups);
            }
        }
    }
    flush(&mut current, &mut groups);
    if let Some(first) = groups.first_mut() {
        first.effective = true;
    }
    PropertyScan { groups }
}

fn push_line(current: &mut Option<PropertyGroup>, line: PropLine) {
    match current {
        Some(g) => {
            g.span.end = line.span.end;
            g.lines.push(line);
        }
        None => {
            *current = Some(PropertyGroup {
                span: line.span,
                lines: vec![line],
                origin: GroupOrigin::Inline,
                effective: false,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::split;

    /// Scans the first block (or the pre-block) of `input`.
    fn scan(input: &str) -> PropertyScan {
        let o = split(input.as_bytes());
        let span = o
            .pre_block
            .or_else(|| o.blocks.first().map(|b| b.span))
            .unwrap_or_default();
        scan_properties(input.as_bytes(), span, ParserOptions::default())
    }

    fn keys(s: &PropertyScan) -> Vec<Vec<(&str, &str)>> {
        s.groups
            .iter()
            .map(|g| {
                g.lines
                    .iter()
                    .map(|l| (l.key_raw.as_str(), l.value_raw.as_str()))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn edge_keys_follow_the_mldoc_rules() {
        let s =
            scan("- a\n  a.b.c:: 1\n  empty::\n  my key:: v\n  key::value\n  k2::  \n  k3:: x  ");
        // `my key:: v` and `key::value` are text and split the run into two groups.
        assert_eq!(
            keys(&s),
            [
                vec![("a.b.c", "1"), ("empty", "")],
                vec![("k2", ""), ("k3", "x")]
            ]
        );
        assert!(s.groups[0].effective && !s.groups[1].effective);
    }

    #[test]
    fn not_properties() {
        for t in [
            "- a\n  :: x",
            "- a\n  a:::: b",
            "- a\n  a::: b",
            "- a\n  a:b:: c",
            "- a\n  a::\tb",
            "- a\n  a: b",
            "- a\n  key::value",
        ] {
            assert!(scan(t).groups.is_empty(), "{t:?}");
        }
    }

    #[test]
    fn late_property_group_is_effective_when_first() {
        let input = "- a\n  text\n  late:: prop";
        let s = scan(input);
        assert_eq!(keys(&s), [vec![("late", "prop")]]);
        assert!(s.effective().is_some_and(|g| g.effective));
        let g = &s.groups[0];
        assert_eq!(g.span.slice(input.as_bytes()), b"  late:: prop");
    }

    #[test]
    fn first_group_wins_and_groups_split_on_text_and_blank_lines() {
        let s = scan("- a\n  a:: 1\n  text\n  b:: 2\n\n  c:: 3");
        assert_eq!(
            keys(&s),
            [vec![("a", "1")], vec![("b", "2")], vec![("c", "3")]]
        );
        assert_eq!(s.groups.iter().filter(|g| g.effective).count(), 1);
        assert!(s.groups[0].effective);
    }

    #[test]
    fn property_on_the_bullet_line_is_a_property() {
        let input = "- a:: b\n  c:: d\n- next";
        let o = split(input.as_bytes());
        let s = scan_properties(input.as_bytes(), o.blocks[0].span, ParserOptions::default());
        assert_eq!(keys(&s), [vec![("a", "b"), ("c", "d")]]);
        assert_eq!(
            s.groups[0].span.slice(input.as_bytes()),
            b"a:: b\n  c:: d\n"
        );
    }

    #[test]
    fn quotes_fences_and_begin_blocks_hide_properties() {
        assert!(scan("- a\n  > a:: b").groups.is_empty());
        assert!(scan("- a\n  ```\n  a:: b\n  ```").groups.is_empty());
        assert!(
            scan("- a\n  #+BEGIN_QUOTE\n  a:: b\n  #+END_QUOTE")
                .groups
                .is_empty()
        );
        // After the region closes, properties are recognised again.
        let s = scan("- a\n  ```\n  x:: y\n  ```\n  k:: v");
        assert_eq!(keys(&s), [vec![("k", "v")]]);
        // An unclosed fence is plain text, so the property after it is real.
        let s = scan("- a\n  ```\n  k:: v");
        assert_eq!(keys(&s), [vec![("k", "v")]]);
    }

    #[test]
    fn directives_join_only_after_a_property() {
        let s = scan("- a\n  a:: 1\n  #+b: 2");
        assert_eq!(keys(&s), [vec![("a", "1"), ("b", "2")]]);
        assert_eq!(s.groups[0].lines[1].kind, PropLineKind::Directive);
        let s = scan("- a\n  #+b: 2\n  a:: 1");
        assert_eq!(keys(&s), [vec![("a", "1")]]);
    }

    #[test]
    fn invalid_keys_are_reported_and_kept() {
        let input = "- a\n  \"x\":: 1\n  (a):: 2\n  #h:: 3\n  ok:: 4";
        let s = scan(input);
        assert_eq!(s.groups.len(), 1);
        let valid: Vec<_> = s.valid_lines().map(|l| l.key_raw.as_str()).collect();
        let invalid: Vec<_> = s.invalid_lines().map(|l| l.key_raw.as_str()).collect();
        assert_eq!(valid, ["ok"]);
        assert_eq!(invalid, ["\"x\"", "(a)", "#h"]);
        // Spans are recorded for every line, valid or not.
        assert_eq!(
            s.groups[0].lines[0].key_span.slice(input.as_bytes()),
            b"\"x\""
        );
        assert_eq!(
            s.groups[0].lines[0].value_span.slice(input.as_bytes()),
            b"1"
        );
    }

    #[test]
    fn keys_are_normalised() {
        assert_eq!(normalize_key("A_B"), "a-b");
        assert_eq!(normalize_key("Custom_ID"), "id");
        assert_eq!(normalize_key("custom-id"), "id");
        assert_eq!(normalize_key("a.b.c"), "a.b.c");
        assert!(is_valid_key("logseq.query/nlp-date"));
        assert!(!is_valid_key("1st"));
        assert!(!is_valid_key("a^b"));
        assert!(!is_valid_key(""));
    }

    #[test]
    fn crlf_lines_exclude_the_cr_from_values() {
        let input = "- a\r\n  k:: v\r\n  l:: w\r\n";
        let s = scan(input);
        assert_eq!(keys(&s), [vec![("k", "v"), ("l", "w")]]);
        let g = &s.groups[0];
        assert_eq!(g.span.slice(input.as_bytes()), b"  k:: v\r\n  l:: w\r\n");
    }

    #[test]
    fn page_properties_in_the_pre_block() {
        let s = scan("title:: My Page\nalias:: Mine, [[My page alias]]\ntags:: project\n\n- first");
        assert_eq!(
            keys(&s),
            [vec![
                ("title", "My Page"),
                ("alias", "Mine, [[My page alias]]"),
                ("tags", "project")
            ]]
        );
    }

    #[test]
    fn bom_is_kept_in_spans_but_excluded_from_keys() {
        let input = "\u{feff}title:: X\r\n\r\n- a\r\n  k:: v\r\n";
        let s = scan(input);
        let l = &s.groups[0].lines[0];
        assert_eq!(l.key_raw, "title");
        assert_eq!(l.value_raw, "X");
        assert_eq!(l.key_span.start, 3);
        // The group span still starts at the first byte, BOM included.
        assert_eq!(s.groups[0].span.start, 0);
        let o = split(input.as_bytes());
        let blk = scan_properties(input.as_bytes(), o.blocks[0].span, ParserOptions::default());
        assert_eq!(keys(&blk), [vec![("k", "v")]]);
    }

    #[test]
    fn spans_are_utf8_byte_offsets() {
        let input = "- é\n  clé:: vé";
        let s = scan(input);
        let l = &s.groups[0].lines[0];
        assert_eq!(l.key_raw, "clé");
        assert_eq!(l.key_span.slice(input.as_bytes()), "clé".as_bytes());
        assert_eq!(l.value_span.slice(input.as_bytes()), "vé".as_bytes());
    }
}
