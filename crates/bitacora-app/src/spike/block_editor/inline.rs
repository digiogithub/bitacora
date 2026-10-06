//! Inline styling of a block's Markdown source (spike-grade tokenizer): dimmed
//! `[[`/`]]`, page refs, `**bold**`, `` `code` `` and task markers. Produces
//!
//! * source-aligned runs for the focused editor (display text == source), and
//! * a display string + offset map for unfocused blocks, where `**`/`` ` `` markers
//!   are hidden and clicks must map back to source offsets (T-0080).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

/// Visual role of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Ordinary text.
    Plain,
    /// `[[` or `]]`: shown dimmed.
    Bracket,
    /// Page name inside `[[ ]]`.
    Ref,
    /// `**` or `` ` ``: dimmed when editing, hidden when rendered.
    Marker,
    /// Text inside `**`.
    Bold,
    /// Text inside backticks.
    Code,
    /// Open task keyword (TODO, DOING, NOW, LATER).
    Todo,
    /// Finished task keyword (DONE, CANCELED).
    Done,
}

/// A styled byte range of the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Source byte range.
    pub range: Range<usize>,
    /// Role.
    pub kind: Kind,
}

const OPEN_TASKS: [&str; 4] = ["TODO", "DOING", "NOW", "LATER"];
const DONE_TASKS: [&str; 2] = ["DONE", "CANCELED"];

/// Tokenizes `text`; tokens are contiguous and cover the whole string.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut push = |range: Range<usize>, kind: Kind| {
        if range.is_empty() {
            return;
        }
        if kind == Kind::Plain
            && let Some(last) = tokens.last_mut()
            && last.kind == Kind::Plain
            && last.range.end == range.start
        {
            last.range.end = range.end;
            return;
        }
        tokens.push(Token { range, kind });
    };

    let mut pos = 0;
    for (words, kind) in [(&OPEN_TASKS[..], Kind::Todo), (&DONE_TASKS[..], Kind::Done)] {
        for word in words {
            if let Some(rest) = text.strip_prefix(word)
                && (rest.is_empty() || rest.starts_with(' '))
            {
                push(0..word.len(), kind);
                pos = word.len();
            }
        }
    }

    let bytes = text.as_bytes();
    while pos < text.len() {
        let rest = &text[pos..];
        if rest.starts_with("[[")
            && let Some(close) = rest[2..].find("]]")
            && close > 0
            && !rest[2..2 + close].contains(['\n', '['])
        {
            push(pos..pos + 2, Kind::Bracket);
            push(pos + 2..pos + 2 + close, Kind::Ref);
            push(pos + 2 + close..pos + 4 + close, Kind::Bracket);
            pos += 4 + close;
        } else if rest.starts_with("**")
            && let Some(close) = rest[2..].find("**")
            && close > 0
        {
            push(pos..pos + 2, Kind::Marker);
            push(pos + 2..pos + 2 + close, Kind::Bold);
            push(pos + 2 + close..pos + 4 + close, Kind::Marker);
            pos += 4 + close;
        } else if bytes[pos] == b'`'
            && let Some(close) = rest[1..].find('`')
            && close > 0
        {
            push(pos..pos + 1, Kind::Marker);
            push(pos + 1..pos + 1 + close, Kind::Code);
            push(pos + 1 + close..pos + 2 + close, Kind::Marker);
            pos += 2 + close;
        } else {
            let ch_len = rest.chars().next().map_or(1, char::len_utf8);
            push(pos..pos + ch_len, Kind::Plain);
            pos += ch_len;
        }
    }
    tokens
}

/// Runs `(byte_len, kind, underline)` aligned with the source, for the focused editor.
/// `marked` (the IME composition) gets the underline flag.
pub fn source_runs(text: &str, marked: Option<Range<usize>>) -> Vec<(usize, Kind, bool)> {
    let mut runs = Vec::new();
    for token in tokenize(text) {
        let mut cuts = vec![token.range.start];
        if let Some(m) = &marked {
            for cut in [m.start, m.end] {
                if cut > token.range.start && cut < token.range.end {
                    cuts.push(cut);
                }
            }
        }
        cuts.push(token.range.end);
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let underline = marked
                .as_ref()
                .is_some_and(|m| pair[0] >= m.start && pair[1] <= m.end);
            runs.push((pair[1] - pair[0], token.kind, underline));
        }
    }
    runs
}

/// One visible piece of an unfocused block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Range in the display string.
    pub display: Range<usize>,
    /// Range in the source.
    pub source: Range<usize>,
    /// Role.
    pub kind: Kind,
}

/// Display text of an unfocused block and its map back to the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRender {
    /// Text as shown (markers hidden).
    pub display: String,
    /// Visible segments in order.
    pub segments: Vec<Segment>,
}

impl InlineRender {
    /// Renders `text` for display.
    pub fn new(text: &str) -> Self {
        let mut display = String::with_capacity(text.len());
        let mut segments = Vec::new();
        for token in tokenize(text) {
            if token.kind == Kind::Marker {
                continue;
            }
            let start = display.len();
            display.push_str(&text[token.range.clone()]);
            segments.push(Segment {
                display: start..display.len(),
                source: token.range,
                kind: token.kind,
            });
        }
        Self { display, segments }
    }

    /// Maps a display byte offset to the source byte offset the caret should take.
    /// A boundary between two segments maps to the end of the earlier one, so a click
    /// right after a bold word lands before its closing marker.
    pub fn display_to_source(&self, offset: usize) -> usize {
        for seg in &self.segments {
            if offset >= seg.display.start && offset <= seg.display.end {
                return seg.source.start + (offset - seg.display.start);
            }
        }
        self.segments.last().map_or(0, |s| s.source.end)
    }

    /// Maps a source offset to the display offset (hidden markers snap to the next
    /// visible character).
    pub fn source_to_display(&self, offset: usize) -> usize {
        for seg in &self.segments {
            if offset <= seg.source.end && offset >= seg.source.start {
                return seg.display.start + (offset - seg.source.start);
            }
            if offset < seg.source.start {
                return seg.display.start;
            }
        }
        self.display.len()
    }
}

/// Bounded cache of [`InlineRender`] by block text (T-0089). GPUI already caches
/// shaped lines frame to frame; this saves re-tokenizing and the display allocation
/// for every visible row on every frame.
#[derive(Debug)]
pub struct InlineCache {
    map: RefCell<HashMap<u64, Rc<InlineRender>>>,
    capacity: usize,
    hits: Cell<u64>,
    misses: Cell<u64>,
}

impl InlineCache {
    /// A cache holding at most `capacity` entries (cleared wholesale when full).
    pub fn new(capacity: usize) -> Self {
        Self {
            map: RefCell::new(HashMap::new()),
            capacity,
            hits: Cell::new(0),
            misses: Cell::new(0),
        }
    }

    /// The render of `text`, computed once per distinct text.
    pub fn get(&self, text: &str) -> Rc<InlineRender> {
        let mut hasher = std::hash::DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();
        if let Some(found) = self.map.borrow().get(&key) {
            self.hits.set(self.hits.get() + 1);
            return found.clone();
        }
        self.misses.set(self.misses.get() + 1);
        let render = Rc::new(InlineRender::new(text));
        let mut map = self.map.borrow_mut();
        if map.len() >= self.capacity {
            map.clear();
        }
        map.insert(key, render.clone());
        render
    }

    /// `(hits, misses)` since creation.
    pub fn stats(&self) -> (u64, u64) {
        (self.hits.get(), self.misses.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, Kind)> {
        tokenize(text)
            .into_iter()
            .map(|t| (&text[t.range], t.kind))
            .collect()
    }

    #[test]
    fn tokenizes_refs_bold_code_and_task_markers() {
        assert_eq!(
            kinds("TODO see [[Page]] **hi** `x`"),
            [
                ("TODO", Kind::Todo),
                (" see ", Kind::Plain),
                ("[[", Kind::Bracket),
                ("Page", Kind::Ref),
                ("]]", Kind::Bracket),
                (" ", Kind::Plain),
                ("**", Kind::Marker),
                ("hi", Kind::Bold),
                ("**", Kind::Marker),
                (" ", Kind::Plain),
                ("`", Kind::Marker),
                ("x", Kind::Code),
                ("`", Kind::Marker),
            ]
        );
        assert_eq!(kinds("DONE"), [("DONE", Kind::Done)]);
        assert_eq!(kinds("TODOS"), [("TODOS", Kind::Plain)]);
    }

    #[test]
    fn unclosed_markup_stays_plain_and_tokens_cover_the_text() {
        for text in [
            "[[open",
            "**open",
            "`open",
            "[[]]",
            "x\u{e9}y \u{1f600} [[\u{6f22}]]",
        ] {
            let tokens = tokenize(text);
            let mut pos = 0;
            for t in &tokens {
                assert_eq!(t.range.start, pos, "gap in {text:?}");
                pos = t.range.end;
            }
            assert_eq!(pos, text.len());
        }
        assert_eq!(kinds("[[open"), [("[[open", Kind::Plain)]);
    }

    #[test]
    fn source_runs_split_at_the_marked_range() {
        let runs = source_runs("ab cd", Some(1..4));
        assert_eq!(runs.iter().map(|r| r.0).sum::<usize>(), 5);
        assert_eq!(
            runs,
            [
                (1, Kind::Plain, false),
                (3, Kind::Plain, true),
                (1, Kind::Plain, false)
            ]
        );
    }

    #[test]
    fn display_hides_markers_and_maps_clicks_to_source() {
        let src = "a **bold** [[R]] z";
        let render = InlineRender::new(src);
        assert_eq!(render.display, "a bold [[R]] z");
        // A boundary maps to the end of the earlier segment: before the opening `**`.
        assert_eq!(render.display_to_source(2), 2);
        assert_eq!(render.display_to_source(3), 5);
        assert_eq!(render.display_to_source(6), 8);
        // Right after "bold" lands before the closing marker, not after it.
        assert_eq!(render.display_to_source(render.display.len()), src.len());
        assert_eq!(render.source_to_display(5), 3);
        assert_eq!(render.source_to_display(src.len()), render.display.len());
    }

    #[test]
    fn leading_hidden_marker_maps_to_the_first_visible_char() {
        let render = InlineRender::new("**b**");
        assert_eq!(render.display, "b");
        assert_eq!(render.display_to_source(0), 2);
        assert_eq!(render.display_to_source(1), 3);
    }

    #[test]
    fn cache_returns_the_same_render_and_counts_hits() {
        let cache = InlineCache::new(2);
        let a = cache.get("x **y**");
        let b = cache.get("x **y**");
        assert!(Rc::ptr_eq(&a, &b));
        cache.get("other");
        cache.get("third");
        assert_eq!(cache.stats(), (1, 3));
    }
}
