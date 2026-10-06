//! Canonical block writer: the form Logseq writes for an edited or new block, minus its quirks.
//!
//! Rules (`docs/analysis/logseq/02-markdown-block-syntax.md` §2.2, §2.3, §2.7, §7), re-implemented
//! from the documented behaviour (ADR-015):
//!
//! * first line: `unit*(depth-1) + "-" + " " + text`; an empty block is a bare `-`;
//! * every following line: `unit*(depth-1) + "  " + text` (a blank inner line becomes the bare
//!   continuation prefix);
//! * the content is trimmed at both edges and split on `\r?\n`; lines are joined with the document
//!   line ending;
//! * a pre-block is `trim(content)` followed by one line ending (the blank line before the first
//!   block comes from the join, see [`crate::serialize`]);
//! * deliberately NOT reproduced: the bullet-less first block for `heading:: true` and the automatic
//!   pre-block conversion of a first block that starts with `key:: `. First blocks keep their `- `.
//! * a Markdown `:PROPERTIES:` drawer is converted to `key:: value` lines (only because the block
//!   is being written, i.e. it was edited).

use crate::lines::ParserOptions;
use crate::properties::{GroupOrigin, scan_properties};
use crate::span::Span;

/// Line ending used when writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Eol {
    /// `\n` (default, and what new files use).
    #[default]
    Lf,
    /// `\r\n`.
    Crlf,
}

impl Eol {
    /// The line ending as text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }

    /// The line ending of the first line break in `input` (`Lf` when there is none).
    #[must_use]
    pub fn detect(input: &[u8]) -> Self {
        match input.iter().position(|&b| b == b'\n') {
            Some(i) if i > 0 && input[i - 1] == b'\r' => Self::Crlf,
            _ => Self::Lf,
        }
    }
}

/// Indentation unit per tree level (`:export/bullet-indentation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndentUnit {
    /// One tab (Logseq's default).
    #[default]
    Tab,
    /// Two spaces.
    TwoSpaces,
    /// Four spaces.
    FourSpaces,
    /// Eight spaces.
    EightSpaces,
}

impl IndentUnit {
    /// The unit as text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tab => "\t",
            Self::TwoSpaces => "  ",
            Self::FourSpaces => "    ",
            Self::EightSpaces => "        ",
        }
    }

    /// Detects the indentation style of a file from its indented bullet lines: a tab as the first
    /// indent character means tabs; otherwise the smallest space indent (2, 4 or 8, anything else
    /// rounds to 2). A file without indented bullets uses the default (tab).
    #[must_use]
    pub fn detect(input: &[u8], outline: &crate::outline::Outline) -> Self {
        let mut min_spaces: Option<usize> = None;
        for b in &outline.blocks {
            let ind = b.indent.slice(input);
            match ind.first() {
                None => {}
                Some(b'\t') => return Self::Tab,
                Some(_) => {
                    min_spaces = Some(min_spaces.map_or(ind.len(), |m| m.min(ind.len())));
                }
            }
        }
        match min_spaces {
            None => Self::Tab,
            Some(4) => Self::FourSpaces,
            Some(8) => Self::EightSpaces,
            Some(_) => Self::TwoSpaces,
        }
    }
}

/// Writes one block (without a trailing line ending) in canonical form. `depth` is 1-based.
#[must_use]
pub fn write_block(content: &str, depth: usize, unit: IndentUnit, eol: Eol) -> String {
    let indent = unit.as_str().repeat(depth.max(1) - 1);
    let converted = convert_drawers(content);
    let body = converted.trim();
    let mut out = format!("{indent}-");
    if body.is_empty() {
        return out;
    }
    out.push(' ');
    let cont = format!("{indent}  ");
    for (i, line) in body.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if i > 0 {
            out.push_str(eol.as_str());
            out.push_str(&cont);
        }
        out.push_str(line);
    }
    out
}

/// Writes a pre-block: the trimmed content with its internal bytes (line endings included) intact.
/// Returns an empty string for a blank pre-block.
#[must_use]
pub fn write_pre_block(content: &str) -> String {
    content.trim().to_owned()
}

/// Converts every Markdown `:PROPERTIES:` drawer in `content` (a de-indented block content) into
/// `key:: value` lines, using the key mapping of the drawer reader. Everything else is untouched.
#[must_use]
pub fn convert_drawers(content: &str) -> String {
    let scan = scan_properties(
        content.as_bytes(),
        Span::new(0, content.len()),
        ParserOptions::default(),
    );
    let mut out = content.to_owned();
    for g in scan
        .groups
        .iter()
        .rev()
        .filter(|g| g.origin == GroupOrigin::Drawer)
    {
        let indent: String = content[g.span.start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let had_eol = content[g.span.range()].ends_with('\n');
        let mut repl = g
            .lines
            .iter()
            .map(|l| {
                if l.value_raw.is_empty() {
                    format!("{indent}{}::", l.key_norm)
                } else {
                    format!("{indent}{}:: {}", l.key_norm, l.value_raw)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if had_eol && !repl.is_empty() {
            repl.push('\n');
        }
        out.replace_range(g.span.range(), &repl);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_and_units() {
        assert_eq!(
            write_block("x", 3, IndentUnit::TwoSpaces, Eol::Lf),
            "    - x"
        );
        assert_eq!(write_block("x", 2, IndentUnit::Tab, Eol::Lf), "\t- x");
        assert_eq!(write_block("", 2, IndentUnit::Tab, Eol::Lf), "\t-");
        assert_eq!(write_block("  \n", 1, IndentUnit::Tab, Eol::Lf), "-");
    }

    #[test]
    fn continuation_lines_and_blank_inner_lines() {
        assert_eq!(
            write_block("a\n\nb\r\nc\n", 2, IndentUnit::Tab, Eol::Crlf),
            "\t- a\r\n\t  \r\n\t  b\r\n\t  c"
        );
    }

    #[test]
    fn heading_first_block_keeps_its_bullet() {
        assert_eq!(
            write_block("Intro\nheading:: true", 1, IndentUnit::Tab, Eol::Lf),
            "- Intro\n  heading:: true"
        );
    }

    #[test]
    fn drawer_becomes_property_lines() {
        let c = "title\n:PROPERTIES:\n:custom_id: 6500c1a4-0000-4000-8000-000000000001\n:Some_Key: v\n:END:\nbody";
        assert_eq!(
            convert_drawers(c),
            "title\nid:: 6500c1a4-0000-4000-8000-000000000001\nsome-key:: v\nbody"
        );
        assert_eq!(
            convert_drawers("a\n:LOGBOOK:\n:END:"),
            "a\n:LOGBOOK:\n:END:"
        );
    }

    #[test]
    fn detects_style() {
        for (src, want) in [
            ("- a\n\t- b\n", IndentUnit::Tab),
            ("- a\n  - b\n    - c\n", IndentUnit::TwoSpaces),
            ("- a\n    - b\n", IndentUnit::FourSpaces),
            ("- a\n", IndentUnit::Tab),
        ] {
            let o = crate::outline::split(src.as_bytes());
            assert_eq!(IndentUnit::detect(src.as_bytes(), &o), want, "{src:?}");
        }
        assert_eq!(Eol::detect(b"a\r\nb\n"), Eol::Crlf);
        assert_eq!(Eol::detect(b"a\nb\r\n"), Eol::Lf);
    }
}
