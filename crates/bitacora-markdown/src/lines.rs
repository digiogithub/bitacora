//! Line scanner: splits the input into lines and classifies each one, tracking the regions
//! (code fences, `#+BEGIN_X` blocks, YAML front matter) whose lines can never start a block.
//!
//! The rules were derived from the documented behaviour of mldoc 1.5.7 (see
//! `docs/analysis/logseq/02-markdown-block-syntax.md` §2.1 and §8) and checked black-box by running
//! mldoc on probe inputs. Nothing here is copied from Logseq or mldoc (ADR-015).
//!
//! Observed rules implemented here:
//!
//! * Lines end at `\n`; a `\r` right before it belongs to the EOL (`eol_len == 2`). A lone `\r` is
//!   ordinary content.
//! * A bullet is `[ \t]*-` followed by a space, a tab or the end of the line. Only ASCII spaces and
//!   tabs count as whitespace (a no-break space does not).
//! * An ATX heading line is `[ \t]*#+` followed by whitespace or the end of the line, at any indent.
//! * A fence opens on a line whose text (after the indent, or after `- ` on a bullet line) starts
//!   with ```` ``` ```` or `~~~`. It is closed by the next line that starts (after optional
//!   indentation) with either marker; the marker kind and run length are not compared.
//! * `#+BEGIN_<NAME>` (case-insensitive, same placement rules) closes at the next line starting with
//!   `#+END_<NAME>` (case-insensitive prefix match).
//! * An opener without a closer is plain text and opens no region
//!   ([`UnclosedRegion::FallbackToParagraph`], the observed mldoc behaviour).
//! * YAML front matter (`---` on the first line up to the next `---` line) is a region; an unclosed
//!   one is not front matter.
//! * A UTF-8 BOM is *not* stripped: it belongs to the first line, exactly as in mldoc, so a bullet on
//!   the first line of a BOM file is not a block.

use crate::span::Span;

/// What to do with a fence or `#+BEGIN_X` opener that has no closer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnclosedRegion {
    /// The region extends to the end of the input (bullets inside never start blocks).
    ToEof,
    /// The opener line is plain text and later bullets still start blocks. This is what mldoc
    /// 1.5.7 does, so it is the default.
    #[default]
    FallbackToParagraph,
}

/// Parser tuning knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParserOptions {
    /// Handling of unclosed fences and `#+BEGIN_X` regions.
    pub unclosed_region: UnclosedRegion,
}

/// Classification of a line that is outside every region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// `[ \t]*-` followed by a space, a tab or the end of the line.
    BulletStart {
        /// Number of indentation bytes (spaces and tabs) before the dash.
        indent_len: usize,
        /// Offset from the line start to the first byte after the dash and the whitespace run that
        /// follows it (the line content end when nothing follows).
        after_dash: usize,
    },
    /// `[ \t]*#+` followed by whitespace or the end of the line (a heading without a bullet).
    AtxHeading {
        /// Number of indentation bytes before the first `#`.
        indent_len: usize,
        /// Number of `#` characters.
        hashes: usize,
    },
    /// Anything else, and every line inside a region.
    Text,
}

/// The kind of region a line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// A ```` ``` ```` or `~~~` fence.
    Fence,
    /// A `#+BEGIN_X` ... `#+END_X` block.
    Begin,
    /// YAML front matter at byte 0.
    FrontMatter,
}

/// Where a line sits inside a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionPart {
    /// The line that opens the region (it may still be a bullet or heading line itself).
    Open,
    /// A line strictly between opener and closer.
    Inside,
    /// The line that closes the region.
    Close,
}

/// One input line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// Offset of the first byte of the line.
    pub start: usize,
    /// Offset one past the EOL (or the end of the input for an unterminated last line).
    pub end: usize,
    /// Length of the EOL: 0 (none), 1 (`\n`) or 2 (`\r\n`).
    pub eol_len: usize,
    /// Leading spaces and tabs (never contains `\r`).
    pub indent: &'a [u8],
    /// The line without its EOL.
    pub content: &'a [u8],
    /// Classification (always [`LineKind::Text`] inside a region, except for the opener line).
    pub kind: LineKind,
    /// Region membership, if any.
    pub region: Option<(RegionKind, RegionPart)>,
}

impl Line<'_> {
    /// Offset one past the last content byte (before the EOL).
    #[must_use]
    pub const fn content_end(&self) -> usize {
        self.end - self.eol_len
    }

    /// The span of the whole line including its EOL.
    #[must_use]
    pub const fn span(&self) -> Span {
        Span::new(self.start, self.end)
    }

    /// True when the line starts a block.
    #[must_use]
    pub const fn is_block_start(&self) -> bool {
        matches!(
            self.kind,
            LineKind::BulletStart { .. } | LineKind::AtxHeading { .. }
        )
    }
}

pub(crate) const fn is_ws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn indent_len(content: &[u8]) -> usize {
    content.iter().take_while(|&&b| is_ws(b)).count()
}

/// Splits `input[pos..]` at the first line: returns `(content_end, end, eol_len)`.
fn raw_line(input: &[u8], pos: usize) -> (usize, usize, usize) {
    match input[pos..].iter().position(|&b| b == b'\n') {
        Some(rel) => {
            let nl = pos + rel;
            if nl > pos && input[nl - 1] == b'\r' {
                (nl - 1, nl + 1, 2)
            } else {
                (nl, nl + 1, 1)
            }
        }
        None => (input.len(), input.len(), 0),
    }
}

fn classify(content: &[u8]) -> LineKind {
    let indent = indent_len(content);
    let rest = &content[indent..];
    match rest.first() {
        Some(b'-') if rest.len() == 1 || is_ws(rest[1]) => {
            let ws = rest[1..].iter().take_while(|&&b| is_ws(b)).count();
            LineKind::BulletStart {
                indent_len: indent,
                after_dash: indent + 1 + ws,
            }
        }
        Some(b'#') => {
            let hashes = rest.iter().take_while(|&&b| b == b'#').count();
            if rest.get(hashes).is_none_or(|&b| is_ws(b)) {
                LineKind::AtxHeading {
                    indent_len: indent,
                    hashes,
                }
            } else {
                LineKind::Text
            }
        }
        _ => LineKind::Text,
    }
}

fn starts_with_fence(text: &[u8]) -> bool {
    text.starts_with(b"```") || text.starts_with(b"~~~")
}

fn is_fence_line(content: &[u8]) -> bool {
    starts_with_fence(&content[indent_len(content)..])
}

/// The `NAME` of a `#+BEGIN_NAME` opener at the start of `text`, if any.
fn begin_name(text: &[u8]) -> Option<&[u8]> {
    const PREFIX: &[u8] = b"#+BEGIN_";
    if text.len() <= PREFIX.len() || !text[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    let rest = &text[PREFIX.len()..];
    let n = rest.iter().take_while(|&&b| !is_ws(b)).count();
    (n > 0).then(|| &rest[..n])
}

fn is_end_line(content: &[u8], name: &[u8]) -> bool {
    const PREFIX: &[u8] = b"#+END_";
    let text = &content[indent_len(content)..];
    text.len() >= PREFIX.len() + name.len()
        && text[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
        && text[PREFIX.len()..PREFIX.len() + name.len()].eq_ignore_ascii_case(name)
}

#[derive(Debug, Clone)]
struct Active {
    kind: RegionKind,
    name: Vec<u8>,
}

/// Iterator over the lines of an input. See the module docs for the classification rules.
#[derive(Debug, Clone)]
pub struct Lines<'a> {
    input: &'a [u8],
    pos: usize,
    opts: ParserOptions,
    region: Option<Active>,
    /// Set once a fence opener was found to have no closer: no later fence can have one either.
    no_fence_closer: bool,
    /// `#+BEGIN_` names (lower-cased) already known to have no closer.
    unclosed_begin: Vec<Vec<u8>>,
}

impl<'a> Lines<'a> {
    /// Starts scanning `input` with the default options.
    #[must_use]
    pub fn new(input: &'a [u8]) -> Self {
        Self::with_options(input, ParserOptions::default())
    }

    /// Starts scanning `input` with explicit options.
    #[must_use]
    pub fn with_options(input: &'a [u8], opts: ParserOptions) -> Self {
        Self {
            input,
            pos: 0,
            opts,
            region: None,
            no_fence_closer: false,
            unclosed_begin: Vec::new(),
        }
    }

    fn has_closer(&mut self, from: usize, kind: RegionKind, name: &[u8]) -> bool {
        match kind {
            RegionKind::Fence if self.no_fence_closer => return false,
            RegionKind::Begin
                if self
                    .unclosed_begin
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(name)) =>
            {
                return false;
            }
            _ => {}
        }
        let mut pos = from;
        while pos < self.input.len() {
            let (content_end, end, _) = raw_line(self.input, pos);
            let content = &self.input[pos..content_end];
            let hit = match kind {
                RegionKind::Fence => is_fence_line(content),
                RegionKind::Begin => is_end_line(content, name),
                RegionKind::FrontMatter => content == b"---",
            };
            if hit {
                return true;
            }
            pos = end;
        }
        match kind {
            RegionKind::Fence => self.no_fence_closer = true,
            RegionKind::Begin => self.unclosed_begin.push(name.to_vec()),
            RegionKind::FrontMatter => {}
        }
        false
    }

    /// Decides whether `text` (the part of an outside-region line that may carry an opener) opens
    /// a region whose closer exists (or is allowed to be missing).
    fn try_open(&mut self, text: &[u8], after: usize) -> Option<Active> {
        let (kind, name) = if starts_with_fence(text) {
            (RegionKind::Fence, Vec::new())
        } else {
            (RegionKind::Begin, begin_name(text)?.to_vec())
        };
        let closed = self.has_closer(after, kind, &name);
        (closed || self.opts.unclosed_region == UnclosedRegion::ToEof)
            .then_some(Active { kind, name })
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;

    fn next(&mut self) -> Option<Line<'a>> {
        if self.pos >= self.input.len() {
            return None;
        }
        let start = self.pos;
        let (content_end, end, eol_len) = raw_line(self.input, start);
        self.pos = end;
        let content = &self.input[start..content_end];
        let indent = &content[..indent_len(content)];
        let mut line = Line {
            start,
            end,
            eol_len,
            indent,
            content,
            kind: LineKind::Text,
            region: None,
        };

        if let Some(active) = self.region.take() {
            let closes = match active.kind {
                RegionKind::Fence => is_fence_line(content),
                RegionKind::Begin => is_end_line(content, &active.name),
                RegionKind::FrontMatter => content == b"---",
            };
            let part = if closes {
                RegionPart::Close
            } else {
                self.region = Some(active.clone());
                RegionPart::Inside
            };
            line.region = Some((active.kind, part));
            return Some(line);
        }

        if start == 0 && content == b"---" && self.has_closer(end, RegionKind::FrontMatter, b"") {
            self.region = Some(Active {
                kind: RegionKind::FrontMatter,
                name: Vec::new(),
            });
            line.region = Some((RegionKind::FrontMatter, RegionPart::Open));
            return Some(line);
        }

        line.kind = classify(content);
        let opener_text = match line.kind {
            LineKind::BulletStart { after_dash, .. } => &content[after_dash..],
            _ => &content[indent.len()..],
        };
        if let Some(active) = self.try_open(opener_text, end) {
            line.region = Some((active.kind, RegionPart::Open));
            self.region = Some(active);
        }
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<LineKind> {
        Lines::new(input.as_bytes()).map(|l| l.kind).collect()
    }

    fn bullet(indent_len: usize, after_dash: usize) -> LineKind {
        LineKind::BulletStart {
            indent_len,
            after_dash,
        }
    }

    #[test]
    fn bullet_rules() {
        assert_eq!(kinds("-foo"), [LineKind::Text]);
        assert_eq!(kinds("- "), [bullet(0, 2)]);
        assert_eq!(kinds("-"), [bullet(0, 1)]);
        assert_eq!(kinds("-\n"), [bullet(0, 1)]);
        assert_eq!(kinds("-\t x"), [bullet(0, 3)]);
        assert_eq!(kinds("\t- d"), [bullet(1, 3)]);
        assert_eq!(kinds("    - c"), [bullet(4, 6)]);
        assert_eq!(kinds("* a\n+ b\n1. c\n--\n---"), [LineKind::Text; 5]);
        // A no-break space is not whitespace for mldoc.
        assert_eq!(kinds("-\u{a0}a"), [LineKind::Text]);
        assert_eq!(kinds("\u{a0}- a"), [LineKind::Text]);
    }

    #[test]
    fn atx_rules() {
        let atx = |indent_len, hashes| LineKind::AtxHeading { indent_len, hashes };
        assert_eq!(kinds("## x"), [atx(0, 2)]);
        assert_eq!(kinds("#"), [atx(0, 1)]);
        assert_eq!(kinds("####### x"), [atx(0, 7)]);
        assert_eq!(kinds("  ## x"), [atx(2, 2)]);
        assert_eq!(kinds("##\tx"), [atx(0, 2)]);
        assert_eq!(kinds("#foo\n#+title: x\n#[[a]]"), [LineKind::Text; 3]);
    }

    #[test]
    fn crlf_lines_report_eol_len_and_keep_cr_out_of_indent() {
        let lines: Vec<_> = Lines::new(b"- a\r\n  b\r\nc\n\r\nlast").collect();
        let eols: Vec<_> = lines.iter().map(|l| l.eol_len).collect();
        assert_eq!(eols, [2, 2, 1, 2, 0]);
        assert_eq!(lines[1].indent, b"  ");
        assert_eq!(lines[1].content, b"  b");
        assert_eq!(lines[3].content, b"");
        assert_eq!(lines[3].indent, b"");
        assert_eq!(lines[4].end, 18);
        let total: usize = lines.iter().map(|l| l.end - l.start).sum();
        assert_eq!(total, 18);
    }

    #[test]
    fn lone_cr_is_not_an_eol() {
        let lines: Vec<_> = Lines::new(b"- a\r- b").collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].eol_len, 0);
    }

    #[test]
    fn fence_hides_bullets_and_properties() {
        let k = kinds("- a\n```\n- x\nfoo:: bar\n[[x]]\n```\n- b");
        assert_eq!(k[0], bullet(0, 2));
        assert!(k[1..6].iter().all(|k| *k == LineKind::Text));
        assert_eq!(k[6], bullet(0, 2));
    }

    #[test]
    fn fence_closer_ignores_marker_kind_and_length() {
        // mldoc 1.5.7: ``` is closed by ~~~ and by a longer run.
        let k = kinds("```\n- b\n~~~\n- c");
        assert_eq!(k[1], LineKind::Text);
        assert_eq!(k[3], bullet(0, 2));
        let k = kinds("```\n- b\n````\n- c");
        assert_eq!(k[3], bullet(0, 2));
        // Indented closer and trailing text are accepted.
        let k = kinds("  ```js\n- b\n``` x\n- c");
        assert_eq!(k[1], LineKind::Text);
        assert_eq!(k[3], bullet(0, 2));
    }

    #[test]
    fn bullet_line_can_open_a_fence() {
        let lines: Vec<_> = Lines::new(b"- ```\n- b\n```\n- c").collect();
        assert_eq!(lines[0].kind, bullet(0, 2));
        assert_eq!(lines[0].region, Some((RegionKind::Fence, RegionPart::Open)));
        assert_eq!(lines[1].kind, LineKind::Text);
        assert_eq!(
            lines[2].region,
            Some((RegionKind::Fence, RegionPart::Close))
        );
        assert_eq!(lines[3].kind, bullet(0, 2));
    }

    #[test]
    fn begin_regions_are_case_insensitive_and_match_by_name() {
        let k = kinds("#+begin_quote\n- x\n#+END_SRC\n- y\n#+END_QUOTE\n- z");
        assert_eq!(k[1], LineKind::Text);
        assert_eq!(k[3], LineKind::Text);
        assert_eq!(k[5], bullet(0, 2));
        // Indented opener and closer.
        let k = kinds("  #+BEGIN_SRC js\n- x\n  #+end_src\n- y");
        assert_eq!(k[1], LineKind::Text);
        assert_eq!(k[3], bullet(0, 2));
        // An empty name is not an opener.
        let k = kinds("#+BEGIN_\n- x\n#+END_\n- y");
        assert_eq!(k[1], bullet(0, 2));
    }

    #[test]
    fn unclosed_regions_fall_back_to_text_by_default() {
        let k = kinds("```\n- b\n- c");
        assert_eq!(k, [LineKind::Text, bullet(0, 2), bullet(0, 2)]);
        let k = kinds("#+BEGIN_QUOTE\n- b");
        assert_eq!(k, [LineKind::Text, bullet(0, 2)]);
    }

    #[test]
    fn unclosed_regions_can_extend_to_eof() {
        let opts = ParserOptions {
            unclosed_region: UnclosedRegion::ToEof,
        };
        let lines: Vec<_> = Lines::with_options(b"```\n- b\n- c", opts).collect();
        assert!(lines[1..].iter().all(|l| l.kind == LineKind::Text));
        assert_eq!(
            lines[2].region,
            Some((RegionKind::Fence, RegionPart::Inside))
        );
    }

    #[test]
    fn regions_do_not_nest() {
        // A fence inside a #+BEGIN region is plain content; the END line closes the block.
        let k = kinds("#+BEGIN_QUOTE\n```\n#+END_QUOTE\n- b\n```\n- c");
        assert_eq!(k[3], bullet(0, 2));
        assert_eq!(k[5], bullet(0, 2));
    }

    #[test]
    fn front_matter_is_a_region_only_when_closed() {
        let lines: Vec<_> = Lines::new(b"---\ntags:\n  - a\n---\n- c").collect();
        assert!(lines[..4].iter().all(|l| !l.is_block_start()));
        assert_eq!(
            lines[3].region,
            Some((RegionKind::FrontMatter, RegionPart::Close))
        );
        assert!(lines[4].is_block_start());
        // Unclosed: a horizontal rule, the list items are blocks.
        let k = kinds("---\ntags:\n  - a");
        assert_eq!(k[2], bullet(2, 4));
        // Not at byte 0: no front matter.
        let k = kinds("x\n---\n- a\n---");
        assert_eq!(k[2], bullet(0, 2));
    }

    #[test]
    fn bom_belongs_to_the_first_line() {
        let k = kinds("\u{feff}- a\n- b");
        assert_eq!(k, [LineKind::Text, bullet(0, 2)]);
    }
}
