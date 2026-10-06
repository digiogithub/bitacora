//! `bitacora-markdown`: Lossless Logseq outline parser and serializer: `serialize(parse(bytes)) == bytes` for every fixture, plus the inline tokenizer. Depends on no other Bitacora crate.
//!
//! Layers (ADR-003, `docs/analysis/logseq/02-markdown-block-syntax.md` §9):
//!
//! 1. [`lines`] / [`outline`] / [`tree`]: a lossless line-based outline splitter. Every block keeps
//!    its raw byte span, so untouched blocks are written back verbatim.
//! 2. [`properties`]: `key:: value` property groups with mldoc's key rules, value interpretation
//!    and the Markdown `:PROPERTIES:` drawer reader.

pub mod canonical;
pub mod classify;
pub mod doc;
pub mod edit;
pub mod image_meta;
pub mod lines;
pub mod outline;
pub mod properties;
pub mod serialize;
pub mod span;
pub mod tree;

pub use canonical::{Eol, IndentUnit, convert_drawers, write_block, write_pre_block};
pub use classify::{
    BlockParts, CARD_KEYS, CanonicalView, DiffClass, IdentityConflict, LineClass, MetadataMerge,
    PartProp, PropClass, Side, canonical_view, canonical_view_of, class_of, classify_diff,
    classify_lines, is_card_key, merge_metadata, parse_block_text, union_logbook,
};
pub use doc::{Document, Node};
pub use lines::{Line, LineKind, Lines, ParserOptions, UnclosedRegion};
pub use outline::{BlockKind, Outline, RawBlock, content_of, pre_block_content, split, split_with};
pub use serialize::{WriteOptions, serialize};
pub use span::Span;
pub use tree::{NodeLinks, build_tree, build_tree_from_levels};

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Placeholder variant until the crate gets real functionality.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-markdown";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
        assert_eq!(Error::NotImplemented("x").to_string(), "not implemented: x");
    }
}
