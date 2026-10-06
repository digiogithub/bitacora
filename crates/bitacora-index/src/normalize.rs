//! Text normalisation for search (`docs/design/sqlite-index-schema.md` §6.1).
//!
//! `normalize` = remove built-in property lines, NFKC, lower-case, optionally strip diacritics
//! (default on, `:feature/enable-search-remove-accents?`). Queries go through the same function.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Version of the normaliser; bump when the output changes (stored in `meta`).
pub const NORMALIZER_VERSION: u32 = 1;

/// Blocks longer than this many characters are truncated for indexing.
pub const DEFAULT_MAX_BLOCK_LEN: usize = 10_000;

/// Normalisation settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizeOptions {
    /// Strip diacritics (`é` becomes `e`).
    pub remove_accents: bool,
    /// Truncate text longer than this many characters.
    pub max_len: usize,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        Self {
            remove_accents: true,
            max_len: DEFAULT_MAX_BLOCK_LEN,
        }
    }
}

/// Built-in property keys whose lines never reach the search text (Logseq
/// `property/remove-built-in-properties`): hidden built-ins and the table/view ones.
const BUILT_IN_KEYS: &[&str] = &[
    "id",
    "custom-id",
    "background-color",
    "heading",
    "collapsed",
    "created-at",
    "updated-at",
    "last-modified-at",
    "query-table",
    "query-properties",
    "query-sort-by",
    "query-sort-desc",
    "ls-type",
    "hl-type",
    "hl-page",
    "hl-stamp",
    "hl-color",
    "logseq.macro-name",
    "logseq.macro-arguments",
    "logseq.order-list-type",
    "logseq.tldraw.page",
    "logseq.tldraw.shape",
    "todo",
    "doing",
    "now",
    "later",
    "done",
    "template-including-parent",
    "exclude-from-graph-view",
    "logseq.color",
    "logseq.table.version",
    "logseq.table.compact",
    "logseq.table.headers",
    "logseq.table.hover",
    "logseq.table.borders",
    "logseq.table.stripes",
    "logseq.table.max-width",
];

/// Whether `key_norm` is a built-in property key stripped from search text.
#[must_use]
pub fn is_search_stripped_key(key_norm: &str) -> bool {
    BUILT_IN_KEYS.contains(&key_norm)
}

/// Removes `key:: value` lines whose key is a built-in. Other text is kept untouched.
#[must_use]
pub fn remove_built_in_properties(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut first = true;
    for line in content.split('\n') {
        if let Some((key, _)) = line.trim_start().split_once("::")
            && !key.is_empty()
            && !key.contains(char::is_whitespace)
            && is_search_stripped_key(&bitacora_markdown::properties::normalize_key(key))
        {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(line);
    }
    out
}

/// NFKC, lower-case and (optionally) diacritic folding.
#[must_use]
pub fn fold(text: &str, remove_accents: bool) -> String {
    let nfkc: String = text.nfkc().collect();
    let lower = nfkc.to_lowercase();
    if remove_accents {
        lower
            .nfd()
            .filter(|c| !is_combining_mark(*c))
            .nfc()
            .collect()
    } else {
        lower
    }
}

/// The search text of a block: built-in property lines removed, then [`fold`], truncated to
/// `opts.max_len` characters. The second value tells whether it was truncated.
#[must_use]
pub fn search_text(content: &str, opts: NormalizeOptions) -> (String, bool) {
    let folded = fold(&remove_built_in_properties(content), opts.remove_accents);
    match folded.char_indices().nth(opts.max_len) {
        Some((i, _)) => (folded[..i].to_owned(), true),
        None => (folded, false),
    }
}

/// Normalises a user query with the same rules (no property stripping, no truncation).
#[must_use]
pub fn normalize_query(q: &str, remove_accents: bool) -> String {
    fold(q, remove_accents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_and_compat_forms() {
        assert_eq!(fold("Éclair CAFÉ", true), "eclair cafe");
        assert_eq!(fold("Éclair", false), "éclair");
        // NFKC: full-width, ligatures and circled digits.
        assert_eq!(fold("ＡＢＣ ﬁne ①", true), "abc fine 1");
        // Decomposed input folds the same as composed.
        assert_eq!(fold("Cafe\u{301}", true), fold("Caf\u{e9}", true));
        // Non-latin scripts are untouched by accent folding.
        assert_eq!(fold("中文 Ωmega", true), "中文 ωmega");
    }

    #[test]
    fn strips_only_built_in_property_lines() {
        let c = "title text\nid:: 6500c1a4-0000-4000-8000-000000000001\ncollapsed:: true\nmine:: keep\nCreated_At:: 5";
        let (s, cut) = search_text(c, NormalizeOptions::default());
        assert!(!cut);
        assert_eq!(s, "title text\nmine:: keep");
    }

    #[test]
    fn truncates_by_chars_and_reports() {
        let opts = NormalizeOptions {
            remove_accents: true,
            max_len: 3,
        };
        let (s, cut) = search_text("ÁÉÍÓÚ", opts);
        assert_eq!(s, "aei");
        assert!(cut);
    }

    #[test]
    fn query_uses_same_folding() {
        assert_eq!(normalize_query("Ünïcode", true), "unicode");
    }
}
