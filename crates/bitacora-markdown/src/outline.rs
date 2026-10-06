//! Block segmentation: splits a page into a pre-block and raw block spans, byte for byte.
//!
//! Every byte of the input belongs to exactly one span: the optional pre-block `[0, first_block)`
//! followed by the blocks, each running from its start line to the start of the next block line.
//! Concatenating the spans therefore reproduces the input (CRLF, BOM, blank lines and all), which is
//! the foundation of the byte-preserving writer (ADR-003, `docs/design/block-editor.md` §5.1).

use std::borrow::Cow;

use crate::lines::{LineKind, Lines, ParserOptions};
use crate::span::Span;

/// How a block starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// A `- ` bullet line.
    Bullet,
    /// An ATX heading line without a bullet (`## x`), always level 1 in the tree.
    AtxHeading {
        /// Number of `#` characters.
        size: usize,
    },
}

/// A block as a set of raw spans into the original input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    /// The whole block: start line up to the start of the next block line (or the input end).
    pub span: Span,
    /// Indentation characters plus one (each space or tab counts 1). Forced to 1 for ATX headings.
    pub raw_level: usize,
    /// The leading indentation bytes of the start line.
    pub indent: Span,
    /// The start line, including its EOL.
    pub head_line: Span,
    /// Everything after the start line (continuation lines, blank lines), possibly empty.
    pub body: Span,
    /// How the block starts.
    pub kind: BlockKind,
}

/// The raw segmentation of a page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outline {
    /// `[0, first_block_start)`, or the whole input when there is no block line. `None` when the
    /// input starts with a block line (or is empty). Includes a BOM and YAML front matter.
    pub pre_block: Option<Span>,
    /// The blocks in document order.
    pub blocks: Vec<RawBlock>,
    /// Length of the input in bytes.
    pub len: usize,
}

impl Outline {
    /// All spans in document order (pre-block first). Their concatenation is the input.
    pub fn spans(&self) -> impl Iterator<Item = Span> + '_ {
        self.pre_block
            .into_iter()
            .chain(self.blocks.iter().map(|b| b.span))
    }
}

/// Splits `input` with the default [`ParserOptions`].
#[must_use]
pub fn split(input: &[u8]) -> Outline {
    split_with(input, ParserOptions::default())
}

/// Splits `input` into a pre-block and blocks.
#[must_use]
pub fn split_with(input: &[u8], opts: ParserOptions) -> Outline {
    let mut starts: Vec<(usize, usize, BlockKind, Span, Span)> = Vec::new();
    for line in Lines::with_options(input, opts) {
        let (level, kind, indent_len) = match line.kind {
            LineKind::BulletStart { indent_len, .. } => {
                (indent_len + 1, BlockKind::Bullet, indent_len)
            }
            LineKind::AtxHeading { indent_len, hashes } => {
                (1, BlockKind::AtxHeading { size: hashes }, indent_len)
            }
            LineKind::Text => continue,
        };
        starts.push((
            line.start,
            level,
            kind,
            Span::new(line.start, line.start + indent_len),
            line.span(),
        ));
    }

    let first = starts.first().map_or(input.len(), |s| s.0);
    let pre_block = (first > 0).then(|| Span::new(0, first));
    let blocks = starts
        .iter()
        .enumerate()
        .map(|(i, &(start, raw_level, kind, indent, head_line))| {
            let end = starts.get(i + 1).map_or(input.len(), |s| s.0);
            RawBlock {
                span: Span::new(start, end),
                raw_level,
                indent,
                head_line,
                body: Span::new(head_line.end, end),
                kind,
            }
        })
        .collect();
    Outline {
        pre_block,
        blocks,
        len: input.len(),
    }
}

/// Semantic content of the pre-block: its text with the leading whitespace removed (and a leading
/// dash run, as Logseq does) but **not** de-indented. Used for meaning only, never for writing.
#[must_use]
pub fn pre_block_content(input: &[u8], span: Span) -> Cow<'_, str> {
    map_str(String::from_utf8_lossy(span.slice(input)), |t| {
        remove_level_spaces(t).trim_end()
    })
}

/// The semantic content of a block: the text Logseq would hold as the block's content.
///
/// Mirrors the documented extraction (`docs/analysis/logseq/02-markdown-block-syntax.md` §2.4):
/// the first line loses its leading whitespace and its `-` run (plus one whitespace character), and
/// every continuation line loses the `raw_level + 1` indentation characters when they are all
/// whitespace (otherwise it is left-trimmed). Line endings become `\n`, and trailing whitespace
/// (including blank lines before the next block) is dropped, as Logseq does on every write.
///
/// This is for semantics (indexing, rendering, editing text) only: untouched blocks are always
/// written back from their raw span. The result borrows from `input` when no rewriting is needed.
#[must_use]
pub fn content_of<'a>(input: &'a [u8], block: &RawBlock) -> Cow<'a, str> {
    let width = block.raw_level + 1;
    let text = String::from_utf8_lossy(block.span.slice(input));
    let stripped = remove_level_spaces(&text);
    let needs_work = stripped.contains('\r')
        || stripped
            .split('\n')
            .skip(1)
            .any(|l| deindent(l, width) != l);
    if !needs_work {
        return map_str(text, |t| remove_level_spaces(t).trim_end());
    }
    let mut out = String::with_capacity(stripped.len());
    for (i, raw) in stripped.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if i == 0 {
            out.push_str(line);
        } else {
            out.push('\n');
            out.push_str(deindent(line, width));
        }
    }
    out.truncate(out.trim_end().len());
    Cow::Owned(out)
}

/// Applies a sub-slicing function to a `Cow<str>` without allocating when it is borrowed.
fn map_str<'a>(text: Cow<'a, str>, f: impl FnOnce(&str) -> &str) -> Cow<'a, str> {
    match text {
        Cow::Borrowed(s) => Cow::Borrowed(f(s)),
        Cow::Owned(s) => Cow::Owned(f(&s).to_owned()),
    }
}

/// Drops the first `width` characters of `line` when they are all whitespace (a shorter blank line
/// becomes empty); otherwise left-trims the line.
fn deindent(line: &str, width: usize) -> &str {
    let mut chars = line.char_indices();
    for _ in 0..width {
        match chars.next() {
            Some((_, c)) if c.is_whitespace() => {}
            Some(_) => return line.trim_start(),
            None => return "",
        }
    }
    chars.next().map_or("", |(i, _)| &line[i..])
}

/// Left-trims, then removes a leading `-` run plus one optional whitespace character. Text that
/// starts with `---` (front matter) is left alone.
fn remove_level_spaces(text: &str) -> &str {
    if text.starts_with("---") {
        return text;
    }
    let rest = text.trim_start().trim_start_matches('-');
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) if c.is_whitespace() => chars.as_str(),
        _ => rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(input: &str, block: usize) -> String {
        let o = split(input.as_bytes());
        content_of(input.as_bytes(), &o.blocks[block]).into_owned()
    }

    fn assert_partition(input: &[u8]) {
        let o = split(input);
        let mut pos = 0;
        for s in o.spans() {
            assert_eq!(s.start, pos, "gap or overlap before {s:?}");
            pos = s.end;
        }
        assert_eq!(pos, input.len());
        let rebuilt: Vec<u8> = o
            .spans()
            .flat_map(|s| s.slice(input).iter().copied())
            .collect();
        assert_eq!(rebuilt, input);
    }

    #[test]
    fn empty_and_pre_block_only() {
        let o = split(b"");
        assert_eq!(o, Outline::default());
        let o = split(b"title:: x\ntext\n");
        assert_eq!(o.pre_block, Some(Span::new(0, 15)));
        assert!(o.blocks.is_empty());
    }

    #[test]
    fn file_starting_with_a_bullet_has_no_pre_block() {
        let o = split(b"- a\n- b");
        assert_eq!(o.pre_block, None);
        assert_eq!(o.blocks.len(), 2);
    }

    #[test]
    fn pre_block_and_blocks_partition_the_input() {
        let input = b"title:: x\n\n- a\r\n  more\r\n\r\n\t- b\n- ";
        assert_partition(input);
        let o = split(input);
        assert_eq!(o.pre_block, Some(Span::new(0, 11)));
        assert_eq!(o.blocks.len(), 3);
        // Blank lines stay with the previous block.
        assert_eq!(o.blocks[0].span.slice(input), b"- a\r\n  more\r\n\r\n");
        assert_eq!(o.blocks[0].head_line.slice(input), b"- a\r\n");
        assert_eq!(o.blocks[0].body.slice(input), b"  more\r\n\r\n");
        assert_eq!(o.blocks[1].raw_level, 2);
        assert_eq!(o.blocks[1].indent.slice(input), b"\t");
        assert_eq!(o.blocks[2].span.slice(input), b"- ");
    }

    #[test]
    fn levels_count_indent_characters() {
        let o = split(b"- a\n  - b\n\t- c\n \t- d\n    - e");
        let levels: Vec<_> = o.blocks.iter().map(|b| b.raw_level).collect();
        assert_eq!(levels, [1, 3, 2, 3, 5]);
    }

    #[test]
    fn atx_headings_are_level_one_blocks() {
        let o = split(b"## hello\n    - world\n  ### deep\n");
        let kinds: Vec<_> = o.blocks.iter().map(|b| (b.kind, b.raw_level)).collect();
        assert_eq!(
            kinds,
            [
                (BlockKind::AtxHeading { size: 2 }, 1),
                (BlockKind::Bullet, 5),
                (BlockKind::AtxHeading { size: 3 }, 1),
            ]
        );
        assert_eq!(split(b"#foo\n- a").blocks.len(), 1);
    }

    #[test]
    fn positions_are_byte_offsets() {
        let input = "- café [[Señor]]\n- b".as_bytes();
        let o = split(input);
        assert_eq!(o.blocks[1].span.start, 19);
    }

    #[test]
    fn bom_stays_in_the_pre_block() {
        let input = "\u{feff}- a\n- b";
        let o = split(input.as_bytes());
        assert_eq!(o.pre_block, Some(Span::new(0, 7)));
        assert_eq!(o.blocks.len(), 1);
        assert_partition(input.as_bytes());
    }

    #[test]
    fn content_strips_bullet_and_indentation() {
        assert_eq!(content("- a\n  b", 0), "a\nb");
        assert_eq!(content("- a\n  b\n\n", 0), "a\nb");
        assert_eq!(content("\t- a\n\t  b\n\t  c", 0), "a\nb\nc");
        assert_eq!(content("-\n", 0), "");
        assert_eq!(content("-   x", 0), "  x");
        assert_eq!(content("## h\n  text", 0), "## h\ntext");
    }

    #[test]
    fn content_handles_irregular_continuation_indent() {
        // Under-indented line is left-trimmed.
        assert_eq!(content("- a\nb", 0), "a\nb");
        assert_eq!(content("\t- a\n b", 0), "a\nb");
        // Extra indentation beyond the bullet text column is kept.
        assert_eq!(content("- a\n     b", 0), "a\n   b");
        // A short whitespace-only line becomes empty.
        assert_eq!(content("\t- a\n \n\t  b", 0), "a\n\nb");
        // Tabs are one character each: two of them are the whole indentation area of level 1.
        assert_eq!(content("- a\n\t\tb", 0), "a\nb");
        assert_eq!(content("- a\n\t\t\tb", 0), "a\n\tb");
    }

    #[test]
    fn content_normalises_crlf() {
        assert_eq!(content("- a\r\n  b\r\n", 0), "a\nb");
    }

    #[test]
    fn content_is_borrowed_when_nothing_changes() {
        let input = b"- a\n  b";
        let o = split(input);
        assert!(matches!(content_of(input, &o.blocks[0]), Cow::Owned(_)));
        let single = b"- one";
        let o = split(single);
        assert!(matches!(
            content_of(single, &o.blocks[0]),
            Cow::Borrowed("one")
        ));
    }

    #[test]
    fn pre_block_content_is_not_deindented() {
        let input = b"title:: x\n  alias:: y\n\n- a";
        let o = split(input);
        let c = pre_block_content(input, o.pre_block.unwrap());
        assert_eq!(c, "title:: x\n  alias:: y");
    }
}
