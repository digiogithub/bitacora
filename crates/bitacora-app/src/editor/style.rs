//! Styling of the block being edited: the Markdown source is shown as written, with the
//! markup dimmed and references coloured. Display text equals source text, so offsets are
//! shared with the buffer.

use std::ops::Range;

use bitacora_markdown::inline::{InlineToken, scan_line};

use crate::ui::text_edit::{Font, FontWeight, StrikethroughStyle, TextRun, UnderlineStyle};
use crate::ui::{ActiveTheme as _, App, Hsla, Pixels, Window, px};

/// Visual role of a source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Ordinary text.
    Plain,
    /// `[[`, `]]`, `((`, `))`: dimmed.
    Bracket,
    /// Page name, tag or block id inside brackets.
    Ref,
    /// `**`, `` ` ``, `~~`: dimmed markup.
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

    let mut line_start = 0;
    for line in text.split('\n') {
        tokenize_line(line, line_start, line_start == 0, &mut push);
        // The line break itself.
        let end = line_start + line.len();
        if end < text.len() {
            push(end..end + 1, Kind::Plain);
        }
        line_start = end + 1;
    }
    tokens
}

fn tokenize_line(line: &str, base: usize, first: bool, push: &mut impl FnMut(Range<usize>, Kind)) {
    let mut pos = 0;
    if first {
        for (words, kind) in [(&OPEN_TASKS[..], Kind::Todo), (&DONE_TASKS[..], Kind::Done)] {
            for word in words {
                if let Some(rest) = line.strip_prefix(word)
                    && (rest.is_empty() || rest.starts_with(' '))
                {
                    push(base..base + word.len(), kind);
                    pos = word.len();
                }
            }
        }
    }
    let mut cursor = pos;
    for token in scan_line(line, 0, line.len()) {
        let span = token.span();
        if span.start < cursor || span.end > line.len() {
            continue;
        }
        gap(line, cursor..span.start, base, push);
        cursor = span.end;
        let at = |a: usize, b: usize| base + a..base + b;
        match &token {
            InlineToken::PageRef(p) => {
                push(at(span.start, p.name.start), Kind::Bracket);
                push(at(p.name.start, p.name.end), Kind::Ref);
                push(at(p.name.end, span.end), Kind::Bracket);
            }
            InlineToken::BlockRef(b) => {
                push(at(span.start, b.id.start), Kind::Bracket);
                push(at(b.id.start, b.id.end), Kind::Ref);
                push(at(b.id.end, span.end), Kind::Bracket);
            }
            InlineToken::Tag(_) | InlineToken::Url(_) | InlineToken::Link(_) => {
                push(at(span.start, span.end), Kind::Ref);
            }
            InlineToken::Code(_) => {
                let raw = &line[span.start..span.end];
                let ticks = raw.bytes().take_while(|b| *b == b'`').count();
                let tail = raw.bytes().rev().take_while(|b| *b == b'`').count();
                if ticks + tail < raw.len() {
                    push(at(span.start, span.start + ticks), Kind::Marker);
                    push(at(span.start + ticks, span.end - tail), Kind::Code);
                    push(at(span.end - tail, span.end), Kind::Marker);
                } else {
                    push(at(span.start, span.end), Kind::Code);
                }
            }
            _ => push(at(span.start, span.end), Kind::Plain),
        }
    }
    gap(line, cursor..line.len(), base, push);
}

/// Plain text between inline tokens: `**bold**` gets markers and weight.
fn gap(line: &str, range: Range<usize>, base: usize, push: &mut impl FnMut(Range<usize>, Kind)) {
    let text = &line[range.clone()];
    let mut pos = 0;
    while pos < text.len() {
        let rest = &text[pos..];
        if rest.starts_with("**")
            && let Some(close) = rest[2..].find("**")
            && close > 0
        {
            let s = base + range.start + pos;
            push(s..s + 2, Kind::Marker);
            push(s + 2..s + 2 + close, Kind::Bold);
            push(s + 2 + close..s + 4 + close, Kind::Marker);
            pos += 4 + close;
        } else {
            let ch = rest.chars().next().map_or(1, char::len_utf8);
            let s = base + range.start + pos;
            push(s..s + ch, Kind::Plain);
            pos += ch;
        }
    }
}

/// Runs `(byte_len, kind, underline)` aligned with the source. `marked` (the IME composition)
/// gets the underline flag.
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

/// Font and size used to shape block text; captured from the element's text style.
#[derive(Debug, Clone)]
pub struct TextMetrics {
    /// Base font.
    pub font: Font,
    /// Font size.
    pub font_size: Pixels,
    /// Row height.
    pub line_height: Pixels,
}

impl TextMetrics {
    /// Reads the inherited text style of the element being laid out.
    pub fn from_window(window: &Window) -> Self {
        let style = window.text_style();
        Self {
            font: style.font(),
            font_size: style.font_size.to_pixels(window.rem_size()),
            line_height: window.line_height(),
        }
    }
}

/// Theme colors used by the editor.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Text.
    pub fg: Hsla,
    /// Dimmed markup.
    pub dim: Hsla,
    /// References.
    pub link: Hsla,
    /// Open task keyword.
    pub todo: Hsla,
    /// Finished task keyword.
    pub done: Hsla,
    /// Selection background.
    pub selection: Hsla,
    /// Caret.
    pub caret: Hsla,
}

impl Palette {
    /// Reads the active theme.
    pub fn from_theme(cx: &App) -> Self {
        let t = cx.theme();
        Self {
            fg: t.foreground,
            dim: t.muted_foreground,
            link: t.info,
            todo: t.primary,
            done: t.success,
            selection: t.selection,
            caret: t.caret,
        }
    }
}

/// Converts kind runs into GPUI text runs.
pub fn style_runs(
    kinds: impl IntoIterator<Item = (usize, Kind, bool)>,
    font: &Font,
    p: &Palette,
) -> Vec<TextRun> {
    kinds
        .into_iter()
        .filter(|(len, _, _)| *len > 0)
        .map(|(len, kind, underline)| {
            let mut font = font.clone();
            let color = match kind {
                Kind::Plain | Kind::Code => p.fg,
                Kind::Bracket | Kind::Marker => p.dim,
                Kind::Ref => p.link,
                Kind::Bold => {
                    font.weight = FontWeight::BOLD;
                    p.fg
                }
                Kind::Todo => {
                    font.weight = FontWeight::BOLD;
                    p.todo
                }
                Kind::Done => {
                    font.weight = FontWeight::BOLD;
                    p.done
                }
            };
            TextRun {
                len,
                font,
                color,
                background_color: (kind == Kind::Code).then(|| p.dim.opacity(0.15)),
                underline: underline.then_some(UnderlineStyle {
                    color: Some(p.fg),
                    thickness: px(1.),
                    wavy: false,
                }),
                strikethrough: (kind == Kind::Done).then_some(StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(p.dim),
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers(text: &str) {
        let tokens = tokenize(text);
        let mut at = 0;
        for t in &tokens {
            assert_eq!(t.range.start, at, "gap in {text:?}");
            at = t.range.end;
        }
        assert_eq!(at, text.len(), "tokens cover {text:?}");
    }

    #[test]
    fn tokens_cover_every_byte() {
        for t in [
            "",
            "plain",
            "TODO [[a]] **b** `c` #t ((6500c1a4-0000-4000-8000-000000000001))\nsecond **x",
            "[[a [[b]]",
            "``` fenced ```",
            "é**é**é",
            "DONE",
            "NOW\n\nx",
        ] {
            covers(t);
        }
    }

    #[test]
    fn roles_are_assigned() {
        let kinds: Vec<_> = tokenize("TODO [[Page]] **b**")
            .into_iter()
            .map(|t| t.kind)
            .collect();
        assert_eq!(kinds[0], Kind::Todo);
        assert!(kinds.contains(&Kind::Bracket));
        assert!(kinds.contains(&Kind::Ref));
        assert!(kinds.contains(&Kind::Bold));
        assert!(kinds.contains(&Kind::Marker));
    }

    #[test]
    fn the_marked_range_is_underlined_across_token_cuts() {
        let runs = source_runs("ab **cd** ef", Some(2..7));
        let total: usize = runs.iter().map(|r| r.0).sum();
        assert_eq!(total, 12);
        assert!(runs.iter().any(|r| r.2));
        assert!(runs.iter().any(|r| !r.2));
    }
}
