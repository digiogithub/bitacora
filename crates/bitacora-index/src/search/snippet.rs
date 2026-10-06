//! Snippet builder (BIT-T-0071): a window of the raw block content around the first match with
//! highlight ranges, built in Rust because FTS5 `snippet()` cannot work with two tokenizers.
//!
//! Matching runs on the folded text (same normalizer as the index) while the snippet shows
//! the original content: a per-character offset map translates folded ranges back.

use std::ops::Range;

use crate::normalize::fold;

/// Default snippet window in characters.
pub const DEFAULT_WINDOW: usize = 160;

/// A display snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// The text to show (may start/end with `…` when cut).
    pub text: String,
    /// Byte ranges into `text` to highlight; sorted, non-overlapping, on char boundaries.
    pub highlights: Vec<Range<usize>>,
}

/// Byte ranges (into `content`) of every occurrence of any of `terms` (already folded),
/// merged and sorted. Matching is case- and (optionally) accent-insensitive.
#[must_use]
pub fn find_matches(content: &str, terms: &[&str], remove_accents: bool) -> Vec<Range<usize>> {
    // folded byte offset -> (orig start, orig end) of the source char; one entry per folded byte.
    let mut folded = String::with_capacity(content.len());
    let mut map: Vec<(usize, usize)> = Vec::with_capacity(content.len());
    let mut buf = [0u8; 4];
    for (i, c) in content.char_indices() {
        let end = i + c.len_utf8();
        let f = fold(c.encode_utf8(&mut buf), remove_accents);
        for _ in 0..f.len() {
            map.push((i, end));
        }
        folded.push_str(&f);
    }
    let mut out: Vec<Range<usize>> = Vec::new();
    for term in terms {
        if term.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(pos) = folded[from..].find(term) {
            let s = from + pos;
            let e = s + term.len();
            if let (Some(first), Some(last)) = (map.get(s), map.get(e - 1)) {
                out.push(first.0..last.1);
            }
            // Advance one char, not one byte, to stay on a boundary.
            from = s + folded[s..].chars().next().map_or(1, char::len_utf8);
        }
    }
    merge(out)
}

fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Builds a snippet of at most about `window` characters around the first match. With no
/// match the snippet is the start of the content.
#[must_use]
pub fn build(content: &str, terms: &[&str], remove_accents: bool, window: usize) -> Snippet {
    let matches = find_matches(content, terms, remove_accents);
    let first = matches.first().map_or(0, |r| r.start);
    let (start, end) = window_bounds(content, first, window.max(8));
    let mut text = String::new();
    if start > 0 {
        text.push('…');
    }
    let prefix = text.len();
    // Newlines would break one-line result rows.
    let body: String = content[start..end]
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    text.push_str(&body);
    if end < content.len() {
        text.push('…');
    }
    let highlights = matches
        .into_iter()
        .filter_map(|r| {
            let s = r.start.max(start);
            let e = r.end.min(end);
            // `' '` and `'\n'` are both one byte, so offsets survive the replacement.
            (s < e).then(|| (s - start + prefix)..(e - start + prefix))
        })
        .collect();
    Snippet { text, highlights }
}

/// Char-boundary window `[start, end)` of about `window` chars containing `anchor`,
/// with roughly a third of it before the anchor.
fn window_bounds(content: &str, anchor: usize, window: usize) -> (usize, usize) {
    let before = window / 3;
    let mut start = anchor;
    for (n, (i, _)) in content[..anchor].char_indices().rev().enumerate() {
        start = i;
        if n + 1 >= before {
            break;
        }
    }
    if content[..anchor].is_empty() {
        start = 0;
    }
    // Prefer starting at a word boundary when we cut mid-text.
    if start > 0
        && let Some(sp) = content[start..anchor].find(char::is_whitespace)
    {
        start += sp
            + content[start + sp..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
    }
    let mut end = content.len();
    for (n, (i, _)) in content[start..].char_indices().enumerate() {
        if n >= window {
            end = start + i;
            break;
        }
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_original_text_through_folding() {
        let s = build("Un café Muy Bueno", &["cafe"], true, 160);
        assert_eq!(s.text, "Un café Muy Bueno");
        assert_eq!(&s.text[s.highlights[0].clone()], "café");
    }

    #[test]
    fn case_insensitive_multiple_terms_and_overlap_merge() {
        let r = find_matches("Foo foobar", &["foo", "foob"], true);
        assert_eq!(r, vec![0..3, 4..8]);
    }

    #[test]
    fn long_content_is_windowed_with_ellipses() {
        let text = format!("{} needle {}", "a ".repeat(200), "b ".repeat(200));
        let s = build(&text, &["needle"], true, 60);
        assert!(s.text.starts_with('…') && s.text.ends_with('…'));
        assert_eq!(&s.text[s.highlights[0].clone()], "needle");
        assert!(s.text.chars().count() < 80, "{}", s.text.chars().count());
    }

    #[test]
    fn no_match_shows_the_start() {
        let s = build("hello world", &["zzz"], true, 160);
        assert_eq!(s.text, "hello world");
        assert!(s.highlights.is_empty());
    }

    #[test]
    fn newlines_become_spaces_and_cjk_works() {
        let s = build("line one\n日本語のテキスト", &["本語"], true, 160);
        assert!(!s.text.contains('\n'));
        assert_eq!(&s.text[s.highlights[0].clone()], "本語");
    }

    #[test]
    fn decomposed_accents_map_back() {
        // "e" + U+0301 combining acute folds to "e".
        let s = build("cafe\u{301} ok", &["cafe"], true, 160);
        assert_eq!(&s.text[s.highlights[0].clone()], "cafe");
    }
}
