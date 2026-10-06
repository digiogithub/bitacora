//! Property value interpretation with Logseq's semantics
//! (`docs/analysis/logseq/02-markdown-block-syntax.md` §3.2).
//!
//! Rules, applied in order: unparsed built-in keys keep the raw string; values wrapped in double
//! quotes stay verbatim (quotes included); page references in the value (`[[x]]`, `#tag`,
//! `#[[a b]]`, nested) form a set of page names, and for `alias` / `aliases` / `tags` (plus the
//! configured keys) the plain text is also split on `,` / `，`; otherwise `true` / `false` become
//! booleans, digit runs become integers, and anything else is a string.
//!
//! The reference scanner here is a small stand-in for the inline scanner (a later story): it knows
//! `[[...]]` (nested), `#tag`, `#[[...]]`, and skips code spans, macros and block refs.

use std::collections::BTreeSet;

/// Settings that influence value interpretation (filled from `config.edn` by a higher layer).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropertyConfig {
    /// Extra keys (normalised) whose values are *not* parsed (`:ignored-page-references-keywords`).
    pub ignored_page_references_keywords: BTreeSet<String>,
    /// Extra keys (normalised) whose plain text is split on commas (`:property/separated-by-commas`).
    pub separated_by_commas: BTreeSet<String>,
}

/// An interpreted property value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropValue {
    /// Built-in or configured key whose value is not parsed (trimmed string).
    Raw(String),
    /// Value wrapped in double quotes; the string includes the quotes.
    Quoted(String),
    /// A set of page names referenced by the value (and comma-separated plain fragments for
    /// `alias`, `aliases`, `tags` and configured keys).
    Pages(BTreeSet<String>),
    /// `true` or `false`.
    Bool(bool),
    /// A non-negative integer.
    Int(i64),
    /// Anything else (trimmed).
    Str(String),
}

/// Built-in keys whose values Logseq never parses (hidden and editable built-ins minus the
/// linkable and the typed ones).
const UNPARSED_BUILT_IN: &[&str] = &[
    "id",
    "custom-id",
    "background-color",
    "background_color",
    "query-properties",
    "query-sort-by",
    "ls-type",
    "hl-type",
    "hl-color",
    "logseq.macro-name",
    "logseq.macro-arguments",
    "logseq.order-list-type",
    "logseq.tldraw.page",
    "logseq.tldraw.shape",
    "title",
    "icon",
    "template",
    "filters",
    "macro",
    "filetags",
    "logseq.color",
    "logseq.table.version",
    "logseq.table.compact",
    "logseq.table.headers",
    "logseq.table.hover",
    "logseq.table.borders",
    "logseq.table.stripes",
    "logseq.table.max-width",
];

/// Keys whose plain text is always split on commas.
const COMMA_SEPARATED: &[&str] = &["alias", "aliases", "tags"];

/// Interprets `value_raw` of the property `key_norm` (a normalised key).
#[must_use]
pub fn interpret(key_norm: &str, value_raw: &str, cfg: &PropertyConfig) -> PropValue {
    let v = value_raw.trim();
    if UNPARSED_BUILT_IN.contains(&key_norm)
        || cfg.ignored_page_references_keywords.contains(key_norm)
    {
        return PropValue::Raw(v.to_owned());
    }
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return PropValue::Quoted(v.to_owned());
    }

    let (mut pages, plains) = scan_refs(v);
    if COMMA_SEPARATED.contains(&key_norm) || cfg.separated_by_commas.contains(key_norm) {
        for plain in &plains {
            pages.extend(
                plain
                    .split([',', '，'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    if !pages.is_empty() {
        return PropValue::Pages(pages);
    }
    match v {
        "true" => PropValue::Bool(true),
        "false" => PropValue::Bool(false),
        _ if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) => v
            .parse::<i64>()
            .map_or_else(|_| PropValue::Str(v.to_owned()), PropValue::Int),
        _ => PropValue::Str(v.to_owned()),
    }
}

/// Scans a property value for page references. Returns the referenced page names and the plain
/// text fragments left between the references (used for comma splitting).
#[must_use]
pub fn scan_refs(value: &str) -> (BTreeSet<String>, Vec<String>) {
    let b = value.as_bytes();
    let mut pages = BTreeSet::new();
    let mut plains = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !plain.is_empty() {
                plains.push(std::mem::take(&mut plain));
            }
        };
    }

    while i < b.len() {
        let rest = &value[i..];
        let prev = value[..i].chars().next_back();
        if b[i] == b'\\' {
            // An escaped character is plain text (`\[[x]]` is not a reference).
            let n = rest.chars().take(2).map(char::len_utf8).sum::<usize>();
            plain.push_str(&rest[..n]);
            i += n;
        } else if b[i] == b'`' {
            let run = rest.bytes().take_while(|&c| c == b'`').count();
            if let Some(end) = find_backtick_run(&rest[run..], run) {
                flush!();
                i += run + end + run;
            } else {
                plain.push_str(&rest[..run]);
                i += run;
            }
        } else if rest.starts_with("{{") {
            let close = if rest.starts_with("{{{") { "}}}" } else { "}}" };
            if let Some(end) = rest.find(close) {
                flush!();
                i += end + close.len();
            } else {
                plain.push_str("{{");
                i += 2;
            }
        } else if rest.starts_with("((") {
            if let Some(end) = rest.find("))") {
                flush!();
                i += end + 2;
            } else {
                plain.push_str("((");
                i += 2;
            }
        } else if rest.starts_with("[[") {
            if let Some((name, len)) = page_ref(rest) {
                flush!();
                add_page(&mut pages, name);
                nested_pages(name, &mut pages);
                i += len;
            } else {
                plain.push_str("[[");
                i += 2;
            }
        } else if b[i] == b'#' && prev.is_none_or(|c| c.is_whitespace() || c == ',') {
            let tail = &rest[1..];
            if tail.starts_with("[[") {
                // `#[[a b]]`: the page reference that follows is the tag.
                flush!();
                i += 1;
            } else if let Some(tag) = hash_tag(tail) {
                flush!();
                add_page(&mut pages, tag);
                i += 1 + tag.len() + trailing_punct_len(tail);
            } else {
                plain.push('#');
                i += 1;
            }
        } else if let Some(c) = rest.chars().next() {
            plain.push(c);
            i += c.len_utf8();
        } else {
            break;
        }
    }
    flush!();
    (pages, plains)
}

fn add_page(pages: &mut BTreeSet<String>, name: &str) {
    let name = name.trim();
    if !name.is_empty() {
        pages.insert(name.to_owned());
    }
}

/// Position of the closing run of exactly `run` backticks in `s`.
fn find_backtick_run(s: &str, run: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            let n = b[i..].iter().take_while(|&&c| c == b'`').count();
            if n == run {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

const TAG_END_PUNCT: &[char] = &[',', ';', '.', '!', '?', '\'', '"', ':'];

/// The tag name at the start of `tail` (the text after `#`), without trailing punctuation.
fn hash_tag(tail: &str) -> Option<&str> {
    let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
    let tag = tail[..end].trim_end_matches(TAG_END_PUNCT);
    (!tag.is_empty()).then_some(tag)
}

fn trailing_punct_len(tail: &str) -> usize {
    let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
    end - tail[..end].trim_end_matches(TAG_END_PUNCT).len()
}

/// Parses a balanced `[[...]]` at the start of `s`. Returns the inner text (the page name) and the
/// total length. Nested references are added by the caller through [`nested_pages`].
fn page_ref(s: &str) -> Option<(&str, usize)> {
    let b = s.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'[' && b[i + 1] == b'[' {
            depth += 1;
            i += 2;
        } else if b[i] == b']' && b[i + 1] == b']' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some((&s[2..i - 2], i));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Adds the page names referenced *inside* a nested reference name.
fn nested_pages(name: &str, pages: &mut BTreeSet<String>) {
    let mut i = 0;
    while let Some(rel) = name[i..].find("[[") {
        let start = i + rel;
        if let Some((inner, len)) = page_ref(&name[start..]) {
            add_page(pages, inner);
            nested_pages(inner, pages);
            i = start + len;
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PropertyConfig {
        PropertyConfig::default()
    }

    fn pages(items: &[&str]) -> PropValue {
        PropValue::Pages(items.iter().map(|s| (*s).to_owned()).collect())
    }

    fn eval(k: &str, v: &str) -> PropValue {
        interpret(k, v, &cfg())
    }

    #[test]
    fn tags_split_on_commas_and_collect_refs() {
        assert_eq!(eval("tags", "foo, bar"), pages(&["foo", "bar"]));
        assert_eq!(eval("tags", "a, [[b c]], #d"), pages(&["a", "b c", "d"]));
        assert_eq!(
            eval("alias", "Mine, [[My page alias]]"),
            pages(&["Mine", "My page alias"])
        );
        assert_eq!(eval("tags", "a，b"), pages(&["a", "b"]));
    }

    #[test]
    fn other_keys_are_not_split() {
        assert_eq!(eval("foo", "a, b"), PropValue::Str("a, b".into()));
        assert_eq!(eval("foo", "a, [[b]]"), pages(&["b"]));
    }

    #[test]
    fn config_can_add_comma_separated_and_ignored_keys() {
        let mut c = cfg();
        c.separated_by_commas.insert("authors".into());
        c.ignored_page_references_keywords.insert("raw".into());
        assert_eq!(interpret("authors", "x, y", &c), pages(&["x", "y"]));
        assert_eq!(
            interpret("raw", "[[x]]", &c),
            PropValue::Raw("[[x]]".into())
        );
    }

    #[test]
    fn quoted_values_stay_verbatim() {
        assert_eq!(
            eval("tags", "\"foo, bar\""),
            PropValue::Quoted("\"foo, bar\"".into())
        );
        assert_eq!(eval("s", "\"1000\""), PropValue::Quoted("\"1000\"".into()));
        assert_eq!(
            eval("s", "\"[[x]]\""),
            PropValue::Quoted("\"[[x]]\"".into())
        );
        assert_eq!(eval("s", "\""), PropValue::Str("\"".into()));
    }

    #[test]
    fn typed_scalars() {
        assert_eq!(eval("n", "1000"), PropValue::Int(1000));
        assert_eq!(eval("n", " 007 "), PropValue::Int(7));
        assert_eq!(eval("b", "true"), PropValue::Bool(true));
        assert_eq!(eval("b", "false"), PropValue::Bool(false));
        assert_eq!(eval("b", "True"), PropValue::Str("True".into()));
        assert_eq!(eval("n", "-1"), PropValue::Str("-1".into()));
        assert_eq!(eval("n", "1.5"), PropValue::Str("1.5".into()));
        assert_eq!(
            eval("n", "99999999999999999999"),
            PropValue::Str("99999999999999999999".into())
        );
        assert_eq!(eval("collapsed", "true"), PropValue::Bool(true));
        assert_eq!(eval("n", ""), PropValue::Str(String::new()));
    }

    #[test]
    fn unparsed_built_ins_keep_the_raw_string() {
        assert_eq!(eval("title", "[[x]]"), PropValue::Raw("[[x]]".into()));
        assert_eq!(
            eval("id", "6500c1a4-0000-4000-8000-000000000001"),
            PropValue::Raw("6500c1a4-0000-4000-8000-000000000001".into())
        );
        assert_eq!(eval("icon", "42"), PropValue::Raw("42".into()));
        // Typed built-ins are parsed like any other value.
        assert_eq!(
            eval("created-at", "1700000000000"),
            PropValue::Int(1_700_000_000_000)
        );
    }

    #[test]
    fn reference_forms() {
        assert_eq!(
            eval("p", "[[a]] and #b and #[[c d]]"),
            pages(&["a", "b", "c d"])
        );
        assert_eq!(eval("p", "[[a [[b]] c]]"), pages(&["a [[b]] c", "b"]));
        assert_eq!(eval("p", "#tag."), pages(&["tag"]));
        assert_eq!(eval("p", "#foo:"), pages(&["foo"]));
        assert_eq!(eval("p", "[label]([[page]])"), pages(&["page"]));
    }

    #[test]
    fn no_refs_inside_code_macros_escapes_or_block_refs() {
        assert_eq!(eval("p", "`[[x]]`"), PropValue::Str("`[[x]]`".into()));
        assert_eq!(
            eval("p", "{{embed [[x]]}}"),
            PropValue::Str("{{embed [[x]]}}".into())
        );
        assert_eq!(eval("p", "\\[[x]]"), PropValue::Str("\\[[x]]".into()));
        assert_eq!(
            eval("p", "((6500c1a4-0000-4000-8000-000000000001))"),
            PropValue::Str("((6500c1a4-0000-4000-8000-000000000001))".into())
        );
        assert_eq!(
            eval("p", "issue#12 a#b"),
            PropValue::Str("issue#12 a#b".into())
        );
        assert_eq!(eval("p", "[[unclosed"), PropValue::Str("[[unclosed".into()));
    }
}
