//! Serializer: originals verbatim, edited nodes in canonical form.

use crate::canonical::{Eol, IndentUnit, write_block, write_pre_block};
use crate::doc::{BOM, Document, Node};

/// Overrides for the detected writing style. `None` keeps what the document detected from its
/// source (LF and tabs for a document parsed from nothing).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Line ending for edited blocks and for the line break inserted between nodes.
    pub eol: Option<Eol>,
    /// Indent unit for edited blocks.
    pub indent: Option<IndentUnit>,
}

/// Serializes `doc`. With no edited node the result equals `doc.source()` byte for byte.
///
/// Boundaries: a line break is inserted before a node when the previous output does not end with
/// one (an original last block without a trailing newline). An edited last block gets a trailing
/// line break only when the source had one, so the file keeps its end-of-file convention.
#[must_use]
pub fn serialize(doc: &Document, opts: &WriteOptions) -> Vec<u8> {
    let eol = opts.eol.unwrap_or_else(|| doc.eol());
    let unit = opts.indent.unwrap_or_else(|| doc.indent_unit());
    let nodes: Vec<&Node> = doc.pre_block.iter().chain(doc.blocks.iter()).collect();
    let src = doc.source();
    let source_ends_with_eol = doc.source_ends_with_eol();
    let mut out: Vec<u8> = Vec::with_capacity(src.len() + 64);

    for (k, node) in nodes.iter().enumerate() {
        let last = k + 1 == nodes.len();
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.extend_from_slice(eol.as_str().as_bytes());
        }
        match node {
            Node::Original { block, .. } => out.extend_from_slice(block.span.slice(src)),
            Node::Pre { span } => out.extend_from_slice(span.slice(src)),
            Node::Edited { depth: 0, content } => {
                let text = write_pre_block(content);
                if !text.is_empty() {
                    out.extend_from_slice(text.as_bytes());
                    out.extend_from_slice(eol.as_str().as_bytes());
                    if !last {
                        // The blank line between the pre-block and the first block.
                        out.extend_from_slice(eol.as_str().as_bytes());
                    }
                }
            }
            Node::Edited { depth, content } => {
                out.extend_from_slice(write_block(content, *depth, unit, eol).as_bytes());
                if !last || source_ends_with_eol {
                    out.extend_from_slice(eol.as_str().as_bytes());
                }
            }
        }
    }
    if doc.has_bom() && !out.starts_with(BOM) {
        let mut with = BOM.to_vec();
        with.extend_from_slice(&out);
        out = with;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ser(d: &Document) -> String {
        String::from_utf8(serialize(d, &WriteOptions::default())).expect("utf8")
    }

    #[test]
    fn untouched_document_is_identical() {
        for s in [
            "",
            "text only",
            "\u{feff}- a\r\n  - b\r\n",
            "- a\n\n\n- b",
            "title:: x\n\n- a\n\t- b  \n",
        ] {
            let d = Document::parse(s);
            assert!(d.is_clean());
            assert_eq!(ser(&d), s);
        }
    }

    #[test]
    fn editing_middle_block_of_crlf_file_keeps_neighbours() {
        let src = "- one\r\n  extra\r\n- two\r\n- three\r\n";
        let mut d = Document::parse(src);
        d.set_block_content(1, "changed\nline2");
        assert_eq!(
            ser(&d),
            "- one\r\n  extra\r\n- changed\r\n  line2\r\n- three\r\n"
        );
    }

    #[test]
    fn unchanged_content_keeps_original_bytes() {
        let src = "-   odd  \n\n\n- b\n";
        let mut d = Document::parse(src);
        let c = d.block_content(0).expect("block").into_owned();
        d.set_block_content(0, &c);
        assert!(d.is_clean());
        assert_eq!(ser(&d), src);
    }

    #[test]
    fn append_after_file_without_trailing_newline() {
        let mut d = Document::parse("- last");
        d.insert_block(1, 1, "new");
        assert_eq!(ser(&d), "- last\n- new");
        let mut d = Document::parse("- last\n");
        d.insert_block(1, 1, "new");
        assert_eq!(ser(&d), "- last\n- new\n");
    }

    #[test]
    fn indent_style_is_detected_and_bom_kept() {
        let mut d = Document::parse("\u{feff}- a\n  - b\n");
        d.insert_block(2, 2, "c");
        assert_eq!(ser(&d), "\u{feff}- a\n  - b\n  - c\n");
        let mut d = Document::parse("\u{feff}title:: x\n\n- a\n");
        d.set_pre_block_content("title:: y");
        assert_eq!(ser(&d), "\u{feff}title:: y\n\n- a\n");
    }

    #[test]
    fn new_page_pre_block_and_empty_block() {
        let mut d = Document::parse("");
        d.set_pre_block_content("tags:: demo");
        d.insert_block(0, 1, "");
        assert_eq!(ser(&d), "tags:: demo\n\n-");
        let mut d = Document::parse("");
        d.set_pre_block_content("tags:: demo");
        assert_eq!(ser(&d), "tags:: demo\n");
    }

    #[test]
    fn two_space_depth_three() {
        let mut d = Document::parse("- a\n  - b\n    - c\n");
        d.insert_block(3, 3, "x");
        assert_eq!(ser(&d), "- a\n  - b\n    - c\n    - x\n");
    }

    #[test]
    fn edited_drawer_block_is_converted_but_untouched_one_is_not() {
        let src = "- a\n  :PROPERTIES:\n  :custom_id: x\n  :END:\n- b\n  :PROPERTIES:\n  :k: v\n  :END:\n";
        let mut d = Document::parse(src);
        assert_eq!(ser(&d), src);
        d.set_block_content(0, "a2\n:PROPERTIES:\n:custom_id: x\n:END:");
        assert_eq!(
            ser(&d),
            "- a2\n  id:: x\n- b\n  :PROPERTIES:\n  :k: v\n  :END:\n"
        );
    }
}
