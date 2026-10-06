//! Property value interpretation with Logseq's semantics
//! (`docs/analysis/logseq/02-markdown-block-syntax.md` §3.2).
//!
//! Rules, applied in order: unparsed built-in keys keep the raw string; values wrapped in double
//! quotes stay verbatim (quotes included); page references in the value (`[[x]]`, `#tag`,
//! `#[[a b]]`, nested) form a set of page names, and for `alias` / `aliases` / `tags` (plus the
//! configured keys) the plain text is also split on `,` / `，`; otherwise `true` / `false` become
//! booleans, digit runs become integers, and anything else is a string.
//!
//! References come from the inline scanner ([`crate::inline`]) in property-value mode.

use std::collections::BTreeSet;

use crate::inline::{RefMode, collect, scan};

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
/// text fragments left between the tokens (used for comma splitting).
///
/// Uses the inline scanner in [`RefMode::PropertyValue`] mode: top-level references only, tags
/// anywhere (`issue#12` references page `12`, as mldoc does), macro arguments scanned, block
/// references and links ignored.
#[must_use]
pub fn scan_refs(value: &str) -> (BTreeSet<String>, Vec<String>) {
    let tokens = scan(value);
    let set = collect(value, &tokens, RefMode::PropertyValue);
    let mut plains = Vec::new();
    let mut at = 0;
    for t in &tokens {
        let s = t.span();
        if s.start > at {
            plains.push(value[at..s.start].to_owned());
        }
        at = at.max(s.end);
    }
    if at < value.len() {
        plains.push(value[at..].to_owned());
    }
    (set.pages.into_iter().collect(), plains)
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
        // A property value keeps a nested reference as one page (mldoc's property refs are
        // top-level only).
        assert_eq!(eval("p", "[[a [[b]] c]]"), pages(&["a [[b]] c"]));
        assert_eq!(eval("p", "#tag."), pages(&["tag"]));
        assert_eq!(eval("p", "#foo:"), pages(&["foo"]));
        assert_eq!(eval("p", "[label]([[page]])"), pages(&["page"]));
    }

    #[test]
    fn no_refs_inside_code_macros_escapes_or_block_refs() {
        assert_eq!(eval("p", "`[[x]]`"), PropValue::Str("`[[x]]`".into()));
        assert_eq!(
            eval("p", "{{cloze a}}"),
            PropValue::Str("{{cloze a}}".into())
        );
        // mldoc scans macro arguments of property values for references.
        assert_eq!(eval("p", "{{embed [[x]]}}"), pages(&["x"]));
        assert_eq!(eval("p", "\\[[x]]"), PropValue::Str("\\[[x]]".into()));
        assert_eq!(
            eval("p", "((6500c1a4-0000-4000-8000-000000000001))"),
            PropValue::Str("((6500c1a4-0000-4000-8000-000000000001))".into())
        );
        // mldoc finds a tag in the middle of a word.
        assert_eq!(eval("p", "issue#12 a#b"), pages(&["12", "b"]));
        assert_eq!(eval("p", "x(#t)"), pages(&["t)"]));
        assert_eq!(eval("p", "[[unclosed"), PropValue::Str("[[unclosed".into()));
    }
}
