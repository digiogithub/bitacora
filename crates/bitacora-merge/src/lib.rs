//! `bitacora-merge`: Block-aware 3-way merge of Logseq pages (ADR-016). Depends only on `bitacora-markdown`.
//!
//! * [`model`]: page model (BIT-US-0049), [`matcher`]: block identity matching.
//! * [`fields`], [`marker`], [`meta`], [`block`]: field-level merge with automatic metadata
//!   resolution (BIT-US-0050).

pub mod block;
pub mod conflict;
pub mod fields;
pub mod lcs;
pub mod marker;
pub mod matcher;
pub mod meta;
pub mod model;
pub mod page;
pub mod structure;

pub use block::{MergedBlock, merge_block};
pub use conflict::{Conflict, ConflictKind, IdRewrite, Note, NoteKind, PageConflict};
pub use fields::{Diff3, FieldResult, diff3, merge_content, merge_user_props};
pub use marker::{TitleParts, marker_rank, merge_planning, merge_title, split_title};
pub use matcher::{Matching, Triple, match_blocks, match_pages};
pub use meta::{MergeEnv, merge_meta};
pub use model::{BlockKey, FileStyle, MergeBlock, MergePage, Meta, PropEntry};
pub use page::{MergeResult, merge_lines, merge_page};

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Placeholder variant until the crate gets real functionality.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-merge";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
        assert_eq!(Error::NotImplemented("x").to_string(), "not implemented: x");
    }
}
