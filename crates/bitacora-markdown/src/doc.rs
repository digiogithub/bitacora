//! Document model with dirty tracking (ADR-003, `docs/design/block-editor.md` byte-preserving
//! serializer).
//!
//! A [`Document`] keeps the original bytes and one [`Node`] per block. Untouched blocks stay
//! [`Node::Original`] / [`Node::Pre`] and are written back verbatim; a block whose content or
//! depth is changed becomes [`Node::Edited`] and goes through the canonical writer.

use std::borrow::Cow;

use crate::canonical::{Eol, IndentUnit};
use crate::outline::{RawBlock, content_of, pre_block_content, split};
use crate::span::Span;
use crate::tree::build_tree;

/// The UTF-8 byte order mark.
pub(crate) const BOM: &[u8] = b"\xef\xbb\xbf";

/// One unit of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A block still backed by its original bytes.
    Original {
        /// The raw block (spans into the source).
        block: RawBlock,
        /// 1-based tree depth at parse time.
        depth: usize,
    },
    /// The original pre-block bytes (`[0, first_block)`).
    Pre {
        /// The pre-block span.
        span: Span,
    },
    /// A new or edited block, written in canonical form. `depth == 0` is a pre-block.
    Edited {
        /// 1-based tree depth (0 for the pre-block).
        depth: usize,
        /// De-indented content (no bullet), as [`content_of`] returns for a parsed block.
        content: String,
    },
}

impl Node {
    /// Tree depth (0 for a pre-block).
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Original { depth, .. } | Self::Edited { depth, .. } => *depth,
            Self::Pre { .. } => 0,
        }
    }

    /// True when the node will be rewritten by the canonical writer.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Edited { .. })
    }
}

/// A parsed page that can be edited and serialized back with minimal byte changes.
#[derive(Debug, Clone)]
pub struct Document {
    source: Vec<u8>,
    /// The pre-block, if any.
    pub pre_block: Option<Node>,
    /// The blocks in document order.
    pub blocks: Vec<Node>,
    eol: Eol,
    unit: IndentUnit,
    bom: bool,
    ends_with_eol: bool,
}

impl Document {
    /// Parses `source`. Never fails: any byte string is a document.
    #[must_use]
    pub fn parse(source: impl Into<Vec<u8>>) -> Self {
        let source = source.into();
        let outline = split(&source);
        let links = build_tree(&outline.blocks);
        let blocks = outline
            .blocks
            .iter()
            .zip(&links)
            .map(|(b, l)| Node::Original {
                block: b.clone(),
                depth: l.depth,
            })
            .collect();
        Self {
            pre_block: outline.pre_block.map(|span| Node::Pre { span }),
            blocks,
            eol: Eol::detect(&source),
            unit: IndentUnit::detect(&source, &outline),
            bom: source.starts_with(BOM),
            ends_with_eol: source.ends_with(b"\n"),
            source,
        }
    }

    /// The original bytes.
    #[must_use]
    pub fn source(&self) -> &[u8] {
        &self.source
    }

    /// Appends `extra` after the parsed source and returns the offset where it starts. Spans of
    /// existing nodes stay valid; a caller can add [`Node::Original`] nodes whose spans point
    /// into the appended bytes to emit blocks that came from an older version of the file
    /// verbatim. Style detection and the end-of-file convention keep describing the parsed
    /// source.
    pub fn extend_source(&mut self, extra: &[u8]) -> usize {
        let at = self.source.len();
        self.source.extend_from_slice(extra);
        at
    }

    /// Whether the parsed source ended with a line break (the end-of-file convention kept when
    /// the last block is rewritten).
    #[must_use]
    pub fn source_ends_with_eol(&self) -> bool {
        self.ends_with_eol
    }

    /// Line ending detected in the source (used for edited blocks).
    #[must_use]
    pub fn eol(&self) -> Eol {
        self.eol
    }

    /// Indent unit detected in the source (used for edited blocks).
    #[must_use]
    pub fn indent_unit(&self) -> IndentUnit {
        self.unit
    }

    /// Whether the source starts with a BOM (kept on write).
    #[must_use]
    pub fn has_bom(&self) -> bool {
        self.bom
    }

    /// True when no node was edited (serialization returns the source unchanged).
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.pre_block
            .iter()
            .chain(&self.blocks)
            .all(|n| !n.is_dirty())
    }

    /// De-indented content of block `i` (`None` when out of range).
    #[must_use]
    pub fn block_content(&self, i: usize) -> Option<Cow<'_, str>> {
        Some(match self.blocks.get(i)? {
            Node::Original { block, .. } => content_of(&self.source, block),
            Node::Edited { content, .. } => Cow::Borrowed(content.as_str()),
            Node::Pre { .. } => return None,
        })
    }

    /// Content of the pre-block.
    #[must_use]
    pub fn pre_block_text(&self) -> Option<Cow<'_, str>> {
        Some(match self.pre_block.as_ref()? {
            Node::Pre { span } => pre_block_content(&self.source, *span),
            Node::Edited { content, .. } => Cow::Borrowed(content.as_str()),
            Node::Original { .. } => return None,
        })
    }

    /// Replaces the content of block `i`. A no-op (the block stays original) when the content is
    /// semantically unchanged. Returns false when `i` is out of range.
    pub fn set_block_content(&mut self, i: usize, content: &str) -> bool {
        let Some(current) = self.block_content(i) else {
            return false;
        };
        if current.trim_end() == content.trim_end() {
            return true;
        }
        let depth = self.blocks[i].depth();
        self.blocks[i] = Node::Edited {
            depth,
            content: content.to_owned(),
        };
        true
    }

    /// Replaces (or creates) the pre-block content, with the same no-op rule.
    pub fn set_pre_block_content(&mut self, content: &str) {
        if self
            .pre_block_text()
            .is_some_and(|c| c.trim() == content.trim())
        {
            return;
        }
        self.pre_block = Some(Node::Edited {
            depth: 0,
            content: content.to_owned(),
        });
    }

    /// Changes the depth of block `i` (a structural edit: the block is rewritten).
    pub fn set_depth(&mut self, i: usize, depth: usize) -> bool {
        let Some(current) = self.block_content(i).map(Cow::into_owned) else {
            return false;
        };
        if self.blocks[i].depth() == depth {
            return true;
        }
        self.blocks[i] = Node::Edited {
            depth: depth.max(1),
            content: current,
        };
        true
    }

    /// Inserts a new block at index `at` (clamped).
    pub fn insert_block(&mut self, at: usize, depth: usize, content: &str) {
        let at = at.min(self.blocks.len());
        self.blocks.insert(
            at,
            Node::Edited {
                depth: depth.max(1),
                content: content.to_owned(),
            },
        );
    }

    /// Removes block `i`.
    pub fn remove_block(&mut self, i: usize) -> bool {
        if i < self.blocks.len() {
            self.blocks.remove(i);
            true
        } else {
            false
        }
    }
}
