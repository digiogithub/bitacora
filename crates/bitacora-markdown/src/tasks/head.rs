//! Block head parser: heading size, task marker and priority of a block's first line.
//!
//! Rules (mldoc 1.5.7, observed with `tools/mldoc-diff`, documented in
//! `docs/analysis/logseq/02-markdown-block-syntax.md` §5.3):
//!
//! * The head is read from the block *content* (the text after the bullet, or an ATX heading line
//!   without a bullet).
//! * `#`s followed by a blank (or the end of the line) make a heading; the size is the number of
//!   `#`s (not capped). `#tag` is a tag, not a heading.
//! * A task marker is only recognised right after the heading hashes (or at the start) and only
//!   when followed by a **space** (`TODOx`, `TODO\tx`, `todo x` and, in a multi-line block,
//!   `TODO` alone on its line are not markers).
//! * A priority `[#X]` (any single character) follows the marker, or the start when there is no
//!   marker. `[#A] TODO x` has a priority but no marker.

use crate::span::Span;

/// A task marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Marker {
    /// `TODO`.
    Todo,
    /// `DOING`.
    Doing,
    /// `DONE`.
    Done,
    /// `LATER`.
    Later,
    /// `NOW`.
    Now,
    /// `WAITING`.
    Waiting,
    /// `WAIT`.
    Wait,
    /// `CANCELED`.
    Canceled,
    /// `CANCELLED`.
    Cancelled,
    /// `STARTED`.
    Started,
    /// `IN-PROGRESS`.
    InProgress,
}

impl Marker {
    /// Every marker mldoc knows.
    pub const ALL: [Marker; 11] = [
        Marker::Todo,
        Marker::Doing,
        Marker::Done,
        Marker::Later,
        Marker::Now,
        Marker::Waiting,
        Marker::Wait,
        Marker::Canceled,
        Marker::Cancelled,
        Marker::Started,
        Marker::InProgress,
    ];

    /// The marker text as written in a file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Marker::Todo => "TODO",
            Marker::Doing => "DOING",
            Marker::Done => "DONE",
            Marker::Later => "LATER",
            Marker::Now => "NOW",
            Marker::Waiting => "WAITING",
            Marker::Wait => "WAIT",
            Marker::Canceled => "CANCELED",
            Marker::Cancelled => "CANCELLED",
            Marker::Started => "STARTED",
            Marker::InProgress => "IN-PROGRESS",
        }
    }

    /// Parses the exact (upper-case) marker word.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Marker> {
        Marker::ALL.into_iter().find(|m| m.as_str() == word)
    }

    /// The next marker in Logseq's click-to-cycle order (`marker.cljs:40-58`) for the TODO/DOING
    /// and LATER/NOW workflows; `None` ends the cycle.
    #[must_use]
    pub const fn cycle(self) -> Option<Marker> {
        match self {
            Marker::Todo => Some(Marker::Doing),
            Marker::Later => Some(Marker::Now),
            Marker::Doing | Marker::Now => Some(Marker::Done),
            Marker::Done => None,
            _ => Some(Marker::Todo),
        }
    }
}

/// The parsed head of a block's first line. All spans are byte offsets into the parsed text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockHead {
    /// Number of `#`s of a heading (`# Title` is 1, `###### x` is 6).
    pub heading: Option<usize>,
    /// The `#`s (the run only, without the blank that follows).
    pub heading_span: Option<Span>,
    /// The task marker.
    pub marker: Option<Marker>,
    /// The marker word.
    pub marker_span: Option<Span>,
    /// The priority character of `[#X]`.
    pub priority: Option<char>,
    /// The whole `[#X]`.
    pub priority_span: Option<Span>,
    /// Offset of the first byte of the title text (after the head and its blanks).
    pub title_start: usize,
}

const fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Parses the head of the first line of `content` (anything after the first line break is
/// ignored).
#[must_use]
pub fn parse_head(content: &str) -> BlockHead {
    let line_end = content.find(['\n', '\r']).unwrap_or(content.len());
    let line = &content[..line_end];
    let b = line.as_bytes();
    let mut head = BlockHead::default();
    let mut i = 0;

    let hashes = b.iter().take_while(|&&c| c == b'#').count();
    if hashes > 0 && (hashes == b.len() || is_blank(b[hashes])) {
        head.heading = Some(hashes);
        head.heading_span = Some(Span::new(0, hashes));
        i = hashes;
        while i < b.len() && is_blank(b[i]) {
            i += 1;
        }
    }

    let word_end = i + b[i..].iter().take_while(|&&c| c != b' ').count();
    if word_end < b.len()
        && let Some(m) = Marker::from_word(&line[i..word_end])
    {
        head.marker = Some(m);
        head.marker_span = Some(Span::new(i, word_end));
        i = word_end;
        while i < b.len() && is_blank(b[i]) {
            i += 1;
        }
    }

    if b[i..].starts_with(b"[#")
        && let Some(c) = line[i + 2..].chars().next()
        && c != ']'
        && b.get(i + 2 + c.len_utf8()) == Some(&b']')
    {
        let end = i + 3 + c.len_utf8();
        head.priority = Some(c);
        head.priority_span = Some(Span::new(i, end));
        i = end;
        while i < b.len() && is_blank(b[i]) {
            i += 1;
        }
    }
    head.title_start = i;
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(s: &str) -> (Option<usize>, Option<Marker>, Option<char>, &str) {
        let h = parse_head(s);
        (h.heading, h.marker, h.priority, &s[h.title_start..])
    }

    #[test]
    fn markers_need_a_following_space() {
        assert_eq!(head("TODO x"), (None, Some(Marker::Todo), None, "x"));
        assert_eq!(head("TODO  x"), (None, Some(Marker::Todo), None, "x"));
        assert_eq!(head("TODO "), (None, Some(Marker::Todo), None, ""));
        assert_eq!(head("TODO"), (None, None, None, "TODO"));
        assert_eq!(head("LATER\n  body"), (None, None, None, "LATER\n  body"));
        assert_eq!(head("TODOx"), (None, None, None, "TODOx"));
        assert_eq!(head("TODO\tx"), (None, None, None, "TODO\tx"));
        assert_eq!(head("todo x"), (None, None, None, "todo x"));
        assert_eq!(head("x TODO y"), (None, None, None, "x TODO y"));
        assert_eq!(
            head("TODO TODO x"),
            (None, Some(Marker::Todo), None, "TODO x")
        );
    }

    #[test]
    fn every_marker_is_recognised() {
        for m in Marker::ALL {
            let line = format!("{} x", m.as_str());
            assert_eq!(parse_head(&line).marker, Some(m), "{line}");
        }
        assert_eq!(head("IN PROGRESS x").1, None);
    }

    #[test]
    fn priorities() {
        assert_eq!(
            head("TODO [#A] x"),
            (None, Some(Marker::Todo), Some('A'), "x")
        );
        assert_eq!(head("[#A] x"), (None, None, Some('A'), "x"));
        assert_eq!(head("[#a] x").2, Some('a'));
        assert_eq!(head("TODO [#1]").2, Some('1'));
        assert_eq!(head("[#AB] x").2, None);
        assert_eq!(head("[#] x").2, None);
        assert_eq!(head("TODO[#A] x").2, None);
        // The marker must come before the priority.
        assert_eq!(head("[#A] TODO x"), (None, None, Some('A'), "TODO x"));
        assert_eq!(head("[#A] [#B] x").2, Some('A'));
        assert_eq!(head("TODO [#é] x").2, Some('é'));
    }

    #[test]
    fn headings() {
        assert_eq!(head("# H"), (Some(1), None, None, "H"));
        assert_eq!(
            head("## TODO [#A] x"),
            (Some(2), Some(Marker::Todo), Some('A'), "x")
        );
        assert_eq!(head("###### x").0, Some(6));
        assert_eq!(head("####### x").0, Some(7));
        assert_eq!(head("#x"), (None, None, None, "#x"));
        assert_eq!(head("#+ x").0, None);
        assert_eq!(head("##").0, Some(2));
        assert_eq!(head("#\tTODO x").1, Some(Marker::Todo));
        assert_eq!(head("# ").3, "");
        assert_eq!(head("").3, "");
    }

    #[test]
    fn spans_point_at_the_head_parts() {
        let s = "## DOING [#B] t";
        let h = parse_head(s);
        assert_eq!(&s[h.heading_span.unwrap().range()], "##");
        assert_eq!(&s[h.marker_span.unwrap().range()], "DOING");
        assert_eq!(&s[h.priority_span.unwrap().range()], "[#B]");
        assert_eq!(&s[h.title_start..], "t");
    }

    #[test]
    fn cycle_order() {
        assert_eq!(Marker::Todo.cycle(), Some(Marker::Doing));
        assert_eq!(Marker::Doing.cycle(), Some(Marker::Done));
        assert_eq!(Marker::Done.cycle(), None);
        assert_eq!(Marker::Later.cycle(), Some(Marker::Now));
        assert_eq!(Marker::Now.cycle(), Some(Marker::Done));
    }
}
