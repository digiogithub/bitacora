//! Autopair rules of the block editor (BIT-T-0141): typing an opening character inserts its
//! partner, typing the partner skips over it, and Backspace between a pair deletes both. The
//! rules follow Logseq's behaviour (`docs/design/block-editor.md` §7.2); the code is our own.
//!
//! Everything is pure: the editor applies the returned [`Edit`] to its buffer.

use std::ops::Range;

/// A text edit and where the selection goes afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Range to replace.
    pub range: Range<usize>,
    /// Replacement text.
    pub insert: String,
    /// Selection after the edit, in offsets of the new text.
    pub selection: Range<usize>,
}

/// The partner of an opening character.
fn partner(ch: char) -> Option<char> {
    Some(match ch {
        '[' => ']',
        '{' => '}',
        '(' => ')',
        '`' => '`',
        '~' => '~',
        '*' => '*',
        '_' => '_',
        '^' => '^',
        '=' => '=',
        '/' => '/',
        '+' => '+',
        _ => return None,
    })
}

/// Characters that only autopair around a selection (typing them plainly stays plain).
fn wraps_only_selection(ch: char) -> bool {
    matches!(ch, '*' | '^' | '_' | '=' | '+' | '/')
}

/// Closing characters that are skipped when they are already next to the caret (the backtick
/// is handled separately).
fn is_skippable_closer(ch: char) -> bool {
    matches!(ch, ']' | '}' | ')' | '~' | '*' | '_' | '^' | '=' | '+' | '/')
}

/// Whether a `(` typed at `at` may autopair: after the start, a line break, a space, `]` or `(`.
fn left_paren_allowed(text: &str, at: usize) -> bool {
    match text[..at].chars().next_back() {
        None => true,
        Some(c) => matches!(c, '\n' | ' ' | ']' | '('),
    }
}

/// What typing `ch` over the selection `sel` of `text` does, or `None` when the character is
/// inserted plainly.
#[must_use]
pub fn on_char(text: &str, sel: Range<usize>, ch: char) -> Option<Edit> {
    let at = sel.start;
    let next = text[sel.end..].chars().next();
    let prev = text[..at].chars().next_back();
    let selected = !sel.is_empty();

    // Typing the partner that already follows the caret moves over it.
    if !selected && is_skippable_closer(ch) && next == Some(ch) {
        let to = at + ch.len_utf8();
        return Some(Edit {
            range: at..at,
            insert: String::new(),
            selection: to..to,
        });
    }
    // A second backtick closes the first one; several in a row each pair (``` fences).
    if !selected && ch == '`' && next == Some('`') && prev != Some('`') {
        let to = at + 1;
        return Some(Edit {
            range: at..at,
            insert: String::new(),
            selection: to..to,
        });
    }
    // `$$` and `^^` open a pair when the first character was typed just before.
    if !selected && (ch == '^' || ch == '$') && prev == Some(ch) && next != Some(ch) {
        let to = at + 2 * ch.len_utf8();
        let mut insert = String::new();
        for _ in 0..4 {
            insert.push(ch);
        }
        return Some(Edit {
            range: at..at,
            insert,
            selection: to..to,
        });
    }
    let close = partner(ch)?;
    if wraps_only_selection(ch) && !selected {
        return None;
    }
    if ch == '(' && !selected && !left_paren_allowed(text, at) {
        return None;
    }
    // Wrap the selection, or insert an empty pair with the caret inside.
    let inner = &text[sel.clone()];
    let mut insert = String::with_capacity(inner.len() + 2);
    insert.push(ch);
    insert.push_str(inner);
    insert.push(close);
    let from = at + ch.len_utf8();
    Some(Edit {
        range: sel,
        insert,
        selection: from..from + inner.len(),
    })
}

/// What Backspace does with the caret at `at` when it sits between a pair (`[|]`, `**|**`):
/// both characters go. `None` otherwise.
#[must_use]
pub fn on_backspace(text: &str, at: usize) -> Option<Edit> {
    let prev = text[..at].chars().next_back()?;
    let next = text[at..].chars().next()?;
    let expected = match prev {
        '$' | ':' => prev,
        other => partner(other)?,
    };
    if next != expected {
        return None;
    }
    let from = at - prev.len_utf8();
    Some(Edit {
        range: from..at + next.len_utf8(),
        insert: String::new(),
        selection: from..from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies an edit like the editor does.
    fn apply(text: &str, e: &Edit) -> (String, Range<usize>) {
        let mut out = text.to_owned();
        out.replace_range(e.range.clone(), &e.insert);
        (out, e.selection.clone())
    }

    fn typed(text: &str, caret: usize, ch: char) -> Option<(String, Range<usize>)> {
        on_char(text, caret..caret, ch).map(|e| apply(text, &e))
    }

    #[test]
    fn opening_brackets_insert_the_pair() {
        assert_eq!(typed("a ", 2, '['), Some(("a []".into(), 3..3)));
        assert_eq!(typed("", 0, '{'), Some(("{}".into(), 1..1)));
        assert_eq!(typed("x", 1, '`'), Some(("x``".into(), 2..2)));
        assert_eq!(typed("x", 1, '~'), Some(("x~~".into(), 2..2)));
    }

    #[test]
    fn left_paren_pairs_only_after_space_start_bracket_or_paren() {
        assert_eq!(typed("", 0, '('), Some(("()".into(), 1..1)));
        assert_eq!(typed("a ", 2, '('), Some(("a ()".into(), 3..3)));
        assert_eq!(typed("[x]", 3, '('), Some(("[x]()".into(), 4..4)));
        assert_eq!(typed("a(", 2, '('), Some(("a(()".into(), 3..3)));
        assert_eq!(typed("f", 1, '('), None, "call syntax stays plain");
    }

    #[test]
    fn emphasis_characters_pair_only_around_a_selection() {
        assert_eq!(typed("a", 1, '*'), None);
        assert_eq!(typed("a", 1, '_'), None);
        let e = on_char("make bold", 5..9, '*').expect("wrap");
        assert_eq!(apply("make bold", &e), ("make *bold*".into(), 6..10));
        let e = on_char("x", 0..1, '[').expect("wrap");
        assert_eq!(apply("x", &e), ("[x]".into(), 1..2));
    }

    #[test]
    fn typing_the_closer_skips_over_it() {
        assert_eq!(typed("[]", 1, ']'), Some(("[]".into(), 2..2)));
        assert_eq!(typed("(a)", 2, ')'), Some(("(a)".into(), 3..3)));
        assert_eq!(typed("**b**", 4, '*'), Some(("**b**".into(), 5..5)));
        assert_eq!(typed("[x", 2, ']'), None, "nothing to skip");
    }

    #[test]
    fn backticks_close_once_then_pair_again_for_fences() {
        assert_eq!(typed("`ab`", 3, '`'), Some(("`ab`".into(), 4..4)));
        // After a backtick, another opens a new pair (```).
        assert_eq!(typed("``", 1, '`'), Some(("````".into(), 2..2)));
    }

    #[test]
    fn doubled_caret_and_dollar_open_a_pair() {
        assert_eq!(typed("a^", 2, '^'), Some(("a^^^^".into(), 3..3)));
        assert_eq!(typed("$", 1, '$'), Some(("$$$$".into(), 2..2)));
        assert_eq!(typed("^^", 1, '^'), None, "no doubling inside ^^");
    }

    #[test]
    fn backspace_between_a_pair_deletes_both() {
        let e = on_backspace("a[]b", 2).expect("pair");
        assert_eq!(apply("a[]b", &e), ("ab".into(), 1..1));
        let e = on_backspace("~~", 1).expect("pair");
        assert_eq!(apply("~~", &e).0, "");
        assert!(on_backspace("[x]", 2).is_none());
        assert!(on_backspace("[", 1).is_none());
        assert!(on_backspace("", 0).is_none());
        let e = on_backspace("$$", 1).expect("dollar");
        assert_eq!(apply("$$", &e).0, "");
    }

    #[test]
    fn multibyte_text_around_the_caret_is_safe() {
        assert_eq!(typed("\u{e9}\u{e9}", 2, '['), Some(("\u{e9}[]\u{e9}".into(), 3..3)));
        let e = on_backspace("\u{6f22}[]", 4).expect("pair");
        assert_eq!(apply("\u{6f22}[]", &e).0, "\u{6f22}");
    }
}
