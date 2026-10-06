//! Pure text helpers: UTF-8 <-> UTF-16 offsets (the platform IME APIs speak UTF-16),
//! grapheme and word boundaries.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

/// Rounds `offset` down to a char boundary and clamps it to the text length.
pub fn clamp_to_boundary(text: &str, offset: usize) -> usize {
    // `str::floor_char_boundary` is stable only since 1.91 (workspace MSRV is 1.90).
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Clamps both ends of a range to char boundaries (and orders them).
pub fn clamp_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_to_boundary(text, range.start);
    let end = clamp_to_boundary(text, range.end).max(start);
    start..end
}

/// UTF-8 byte offset to UTF-16 code-unit offset.
pub fn offset_to_utf16(text: &str, offset: usize) -> usize {
    let end = clamp_to_boundary(text, offset);
    text[..end].chars().map(char::len_utf16).sum()
}

/// UTF-16 code-unit offset to UTF-8 byte offset. An offset in the middle of a
/// surrogate pair rounds up to the end of that char.
pub fn offset_from_utf16(text: &str, offset16: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for ch in text.chars() {
        if utf16 >= offset16 {
            break;
        }
        utf16 += ch.len_utf16();
        utf8 += ch.len_utf8();
    }
    utf8
}

/// UTF-8 range to UTF-16 range.
pub fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

/// UTF-16 range to UTF-8 range.
pub fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range.start)..offset_from_utf16(text, range.end)
}

/// Start of the grapheme cluster before `offset` (0 at the start).
pub fn prev_grapheme(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

/// End of the grapheme cluster at `offset` (text length at the end).
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

/// Start of the word before `offset`, skipping whitespace and punctuation runs.
pub fn prev_word(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .rev()
        .find(|(idx, seg)| *idx < offset && seg.chars().any(char::is_alphanumeric))
        .map_or(0, |(idx, _)| idx)
}

/// End of the word after `offset`.
pub fn next_word(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .find(|(idx, seg)| idx + seg.len() > offset && seg.chars().any(char::is_alphanumeric))
        .map_or(text.len(), |(idx, seg)| idx + seg.len())
}

/// The word (or whitespace/punctuation run) around `offset`, for double-click.
pub fn word_range_at(text: &str, offset: usize) -> Range<usize> {
    let offset = clamp_to_boundary(text, offset);
    for (idx, seg) in text.split_word_bound_indices() {
        if offset >= idx && offset < idx + seg.len() {
            return idx..idx + seg.len();
        }
    }
    text.len()..text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    // "a" (1B/1U) "é" (2B/1U) "漢" (3B/1U) "😀" (4B/2U) "b"
    const MIXED: &str = "a\u{e9}\u{6f22}\u{1f600}b";

    #[test]
    fn utf8_utf16_round_trip_on_mixed_widths() {
        let boundaries = [0usize, 1, 3, 6, 10, 11];
        let utf16 = [0usize, 1, 2, 3, 5, 6];
        for (b, u) in boundaries.iter().zip(utf16) {
            assert_eq!(offset_to_utf16(MIXED, *b), u);
            assert_eq!(offset_from_utf16(MIXED, u), *b);
        }
    }

    #[test]
    fn mid_surrogate_pair_rounds_up_and_mid_char_rounds_down() {
        assert_eq!(offset_from_utf16(MIXED, 4), 10);
        assert_eq!(offset_to_utf16(MIXED, 7), 3);
        assert_eq!(offset_to_utf16(MIXED, 999), 6);
        assert_eq!(offset_from_utf16(MIXED, 999), MIXED.len());
    }

    #[test]
    fn graphemes_move_over_clusters() {
        let text = "e\u{301}x\u{1f1ea}\u{1f1f8}"; // e + combining acute, x, flag
        assert_eq!(next_grapheme(text, 0), 3);
        assert_eq!(next_grapheme(text, 3), 4);
        assert_eq!(next_grapheme(text, 4), text.len());
        assert_eq!(prev_grapheme(text, text.len()), 4);
        assert_eq!(prev_grapheme(text, 3), 0);
        assert_eq!(prev_grapheme(text, 0), 0);
    }

    #[test]
    fn word_motion_skips_spaces() {
        let text = "hello big  world";
        assert_eq!(next_word(text, 0), 5);
        assert_eq!(next_word(text, 5), 9);
        assert_eq!(prev_word(text, text.len()), 11);
        assert_eq!(prev_word(text, 11), 6);
        assert_eq!(word_range_at(text, 7), 6..9);
    }
}
