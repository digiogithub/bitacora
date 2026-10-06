//! Index-ready rows derived from one graph file (`docs/design/sqlite-index-schema.md` §4.3).
//!
//! [`ParsedFile`] is plain data: no DB handle, no I/O. The writer (US-0006) turns it into rows.
//! Block indices (`ord`, `parent_ord`, `subtree_end`) are pre-order positions within the file,
//! the pre-block (when there is one) being `ord` 0.

use std::collections::BTreeSet;

/// Format of the parsed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// Markdown (`.md`, `.markdown`).
    Markdown,
    /// Org-mode (`.org`): only the page is derived, blocks are not parsed yet.
    Org,
    /// Anything else that reaches the parser (whiteboards, EDN, ...): no blocks.
    Other,
}

impl FileFormat {
    /// The value stored in `files.format` / `pages.format`.
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Markdown => Some("markdown"),
            Self::Org => Some("org"),
            Self::Other => None,
        }
    }
}

/// Kind of a page reference (`block_page_refs.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PageRefKind {
    /// `[[link]]` (also nested links).
    Link = 1,
    /// `#tag` / `#[[tag]]`.
    Tag = 2,
    /// A page named by a property value.
    PropertyValue = 3,
    /// A property name (unless disabled or excluded).
    PropertyName = 4,
    /// The task marker (`TODO` ...).
    Marker = 5,
    /// The priority (`A`, `B`, `C`).
    Priority = 6,
    /// A namespace parent of a referenced page.
    NamespaceParent = 7,
    /// `{{embed [[page]]}}`.
    Embed = 8,
}

/// Kind of a block reference (`block_block_refs.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum BlockRefKind {
    /// `((uuid))`.
    Ref = 1,
    /// `{{embed ((uuid))}}`.
    Embed = 2,
    /// `[label](((uuid)))`.
    Link = 3,
}

/// A page name as referenced: the lower-cased key and the spelling written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageRefName {
    /// `page-name-sanity-lc` (`pages.name`).
    pub name: String,
    /// The first spelling seen (`pages.original_name` for placeholders).
    pub original: String,
}

/// A page reference of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPageRef {
    /// The referenced page.
    pub page: PageRefName,
    /// Why the block references it.
    pub kind: PageRefKind,
}

/// A block reference of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBlockRef {
    /// The referenced block UUID, lower-cased (may dangle).
    pub target_uuid: String,
    /// The form of the reference.
    pub kind: BlockRefKind,
}

/// Value type of a property (`block_properties.value_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueType {
    /// Text (also quoted and unparsed built-in values).
    String = 0,
    /// Non-negative integer.
    Integer = 1,
    /// `true` / `false`.
    Boolean = 2,
    /// A set of page references.
    Refs = 3,
}

/// One element of a property value (`block_property_values`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPropertyValue {
    /// Lower-cased NFC text; the page key for refs.
    pub value_norm: String,
    /// The number for integers and 0/1 for booleans.
    pub value_num: Option<f64>,
    /// The referenced page, for ref sets.
    pub ref_page: Option<PageRefName>,
}

/// A typed property (`block_properties` + `block_property_values`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedProperty {
    /// Normalised key (lower-case, `_` and space to `-`).
    pub key: String,
    /// Position among the block's properties.
    pub pos: u32,
    /// The key as written.
    pub raw_key: String,
    /// The trimmed value as written.
    pub raw_value: String,
    /// Value type.
    pub value_type: ValueType,
    /// 0 user, 1 editable built-in, 2 hidden built-in.
    pub builtin: u8,
    /// Value elements (one for scalars, one per page for ref sets).
    pub values: Vec<ParsedPropertyValue>,
}

/// One block (or the pre-block).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBlock {
    /// Pre-order index in the file.
    pub ord: u32,
    /// Largest `ord` in this block's subtree.
    pub subtree_end: u32,
    /// 1 for top-level blocks.
    pub depth: u32,
    /// 0-based position among siblings (the pre-block counts as the first top-level block).
    pub sibling_idx: u32,
    /// `ord` of the parent, `None` for top-level blocks.
    pub parent_ord: Option<u32>,
    /// The page properties block (text before the first bullet).
    pub is_pre_block: bool,
    /// Logseq's `:block/content`: raw text, de-indented, `\n` line endings.
    pub content: String,
    /// First line without heading hashes, marker and priority.
    pub title: String,
    /// Normalised text for FTS (see [`crate::normalize`]).
    pub search_text: String,
    /// Task marker word.
    pub marker: Option<String>,
    /// Priority letter.
    pub priority: Option<String>,
    /// `SCHEDULED` as `yyyymmdd`.
    pub scheduled: Option<u32>,
    /// The `SCHEDULED` timestamp as written (`<2024-01-01 Mon .+1d>`).
    pub scheduled_raw: Option<String>,
    /// `DEADLINE` as `yyyymmdd`.
    pub deadline: Option<u32>,
    /// The `DEADLINE` timestamp as written.
    pub deadline_raw: Option<String>,
    /// The lifted timestamp has a repeater.
    pub repeated: bool,
    /// `collapsed:: true`.
    pub collapsed: bool,
    /// Heading size (`## x` or `heading:: 2`).
    pub heading: Option<u8>,
    /// A valid `id::` UUID (lower-cased).
    pub explicit_uuid: Option<String>,
    /// `created-at::` when an integer (ms).
    pub created_at: Option<i64>,
    /// `updated-at::` when an integer (ms).
    pub updated_at: Option<i64>,
    /// Start of the block in the file bytes (BOM included in offsets).
    pub byte_start: u64,
    /// End of the block in the file bytes.
    pub byte_end: u64,
    /// 1-based line of the first line of the block.
    pub line_start: u32,
    /// `blake3(content)[..16]`.
    pub content_hash: [u8; 16],
    /// Properties in order.
    pub properties: Vec<ParsedProperty>,
    /// Page references (a page can appear once per kind).
    pub page_refs: Vec<ParsedPageRef>,
    /// Block references.
    pub block_refs: Vec<ParsedBlockRef>,
}

/// The page a file defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDef {
    /// `pages.name`.
    pub name: String,
    /// Display title.
    pub original_name: String,
    /// `yyyymmdd` for journals.
    pub journal_day: Option<u32>,
    /// Namespace parents, outermost first (`a/b/c` gives `a`, `a/b`).
    pub namespace_parents: Vec<PageRefName>,
    /// `alias::` pages.
    pub aliases: Vec<PageRefName>,
    /// `tags::` pages.
    pub tags: Vec<PageRefName>,
    /// File format.
    pub format: FileFormat,
    /// `created-at::` of the page properties (ms).
    pub created_at: Option<i64>,
    /// `updated-at::` of the page properties (ms).
    pub updated_at: Option<i64>,
}

/// Severity of a diagnostic (`diagnostics.severity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Severity {
    /// Informational.
    Info = 0,
    /// Something Logseq would also mishandle.
    Warning = 1,
    /// The file could not be indexed fully.
    Error = 2,
}

/// Kind of a diagnostic (`diagnostics.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// The file (or part of it) could not be parsed (invalid UTF-8 was replaced).
    ParseError,
    /// The same `id::` twice in a file.
    DuplicateBlockId,
    /// A property Logseq rejects, or an `id::` that is not a UUID.
    InvalidProperty,
    /// A block was cut for indexing.
    TooLarge,
    /// The file is not parsed (org-mode, EDN, ...).
    Unsupported,
}

impl DiagnosticKind {
    /// Stable name for storage and snapshots.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::DuplicateBlockId => "duplicate_block_id",
            Self::InvalidProperty => "invalid_property",
            Self::TooLarge => "too_large",
            Self::Unsupported => "unsupported",
        }
    }
}

/// A problem found while parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Kind.
    pub kind: DiagnosticKind,
    /// Severity.
    pub severity: Severity,
    /// 1-based line.
    pub line: Option<u32>,
    /// Message.
    pub message: String,
}

/// Everything the index derives from one file.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFile {
    /// The page this file defines.
    pub page: PageDef,
    /// Blocks in pre-order.
    pub blocks: Vec<ParsedBlock>,
    /// Every page name to upsert: referenced pages, namespace parents, property names, markers,
    /// priorities, aliases and tags of the page; first-seen order, no duplicates.
    pub referenced_pages: Vec<PageRefName>,
    /// Problems found.
    pub diagnostics: Vec<Diagnostic>,
    /// `blake3(bytes)` of the whole file.
    pub file_hash: [u8; 32],
}

impl ParsedFile {
    /// The ancestors of `ord` (nearest first), by walking `parent_ord`.
    #[must_use]
    pub fn ancestors(&self, ord: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = self.blocks.get(ord as usize).and_then(|b| b.parent_ord);
        while let Some(p) = cur {
            out.push(p);
            cur = self.blocks.get(p as usize).and_then(|b| b.parent_ord);
        }
        out
    }

    /// Logseq's `:block/path-refs` of a block: its own page refs, the page refs of every ancestor
    /// and the page itself. Sorted, no duplicates (page keys).
    #[must_use]
    pub fn path_refs(&self, ord: u32) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        out.insert(self.page.name.clone());
        let chain = std::iter::once(ord).chain(self.ancestors(ord));
        for o in chain {
            if let Some(b) = self.blocks.get(o as usize) {
                out.extend(b.page_refs.iter().map(|r| r.page.name.clone()));
            }
        }
        out
    }
}
