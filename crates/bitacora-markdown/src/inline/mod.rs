//! Layer 3: the inline scanner (`docs/analysis/logseq/02-markdown-block-syntax.md` §5, §9).
//!
//! A small hand-written scanner that finds exactly the inline constructs Logseq derives meaning
//! from: page references (nested too), tags, block references, labelled links, macros (embeds and
//! queries included) and the constructs that *suppress* references: code spans, math, inline HTML,
//! bare URLs and backslash escapes. Everything else (emphasis, lists, tables, ...) is opaque body
//! text and is left to a renderer. The scanner is read-only: tokens carry byte spans into the
//! scanned text and nothing is rewritten.
//!
//! Behaviour was established by running mldoc 1.5.7 as an oracle (`tools/mldoc-diff/inline.js`);
//! `fixtures/markdown/inline/` holds the corpus and the recorded oracle output.
//!
//! * [`scan`] tokenizes text line by line (inline constructs never span lines).
//! * [`refs`] turns tokens into the reference sets Logseq computes (`with-page-refs`).

pub mod refs;
pub mod scan;

pub use refs::{
    LinkRef, LinkRefKind, MacroCall, RefMode, RefSet, collect, is_asset_or_draw, namespace_parents,
};
pub use scan::{
    BlockRef, InlineToken, Link, LinkTarget, Macro, PageRef, Tag, is_uuid, scan, scan_line,
};
