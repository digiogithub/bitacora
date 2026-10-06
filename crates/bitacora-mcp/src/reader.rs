//! Read-side abstraction between MCP tools and the graph.
//!
//! The server only sees [`GraphReader`]; [`crate::IndexGraphReader`] implements it over the SQLite
//! index (decision: the adapter lives in this crate because `mcp -> index` is an allowed edge in
//! `xtask/src/deps.rs`, and the trait keeps tools testable without a database).
//! Every method is synchronous and called from `spawn_blocking`.
//!
//! Methods other than [`GraphReader::graph_info`] default to a `NotSupported` error so small
//! readers (such as [`StaticGraphReader`]) stay valid.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Summary of the active graph (`get_graph_info` tool).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GraphInfo {
    /// Graph display name.
    pub name: String,
    /// Absolute graph folder path.
    pub path: String,
    /// Number of pages, when known.
    pub page_count: Option<u64>,
    /// Number of blocks, when known.
    pub block_count: Option<u64>,
}

/// Why a read failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderErrorKind {
    /// The reader does not implement this operation.
    NotSupported,
    /// The input is not acceptable (bad path, bad date).
    Invalid,
    /// Storage failure.
    Internal,
}

/// Failure reading from the graph.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ReaderError {
    /// Failure class.
    pub kind: ReaderErrorKind,
    /// Human-readable message.
    pub message: String,
}

impl ReaderError {
    /// A storage failure.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ReaderErrorKind::Internal,
            message: message.into(),
        }
    }
    /// Unacceptable input.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ReaderErrorKind::Invalid,
            message: message.into(),
        }
    }
    /// Operation not implemented by this reader.
    pub fn unsupported(what: &str) -> Self {
        Self {
            kind: ReaderErrorKind::NotSupported,
            message: format!("`{what}` is not supported by this graph reader"),
        }
    }
}

/// Result alias of the reader.
pub type ReaderResult<T> = Result<T, ReaderError>;

/// A page as exposed to agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageInfo {
    /// Normalised (lower-case) name.
    pub name: String,
    /// Display name.
    pub original_name: String,
    /// Page UUID.
    pub uuid: String,
    /// Page properties (the first block), raw values.
    pub properties: BTreeMap<String, String>,
    /// `alias::` values.
    pub aliases: Vec<String>,
    /// `tags::` values.
    pub tags: Vec<String>,
    /// `yyyy-mm-dd` for journal pages.
    pub journal_day: Option<String>,
    /// Graph-relative path of the defining file.
    pub file: Option<String>,
    /// Last update, unix milliseconds.
    pub updated_at: Option<i64>,
    /// Number of blocks.
    pub block_count: u64,
    /// Opaque content version of the page file; changes whenever the file changes.
    pub etag: String,
    /// Internal page id (never serialised to agents).
    #[serde(skip)]
    #[schemars(skip)]
    pub id: i64,
}

/// A block as exposed to agents. `children` is filled by tree tools only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlockInfo {
    /// Block UUID.
    pub uuid: String,
    /// Raw Logseq block content (first line plus continuation lines, without the leading `- `).
    pub content: String,
    /// Display name of the owning page.
    pub page: String,
    /// Block properties by raw key.
    pub properties: BTreeMap<String, String>,
    /// Task marker.
    pub marker: Option<String>,
    /// Priority `A`/`B`/`C`.
    pub priority: Option<String>,
    /// `yyyy-mm-dd`.
    pub scheduled: Option<String>,
    /// `yyyy-mm-dd`.
    pub deadline: Option<String>,
    /// `collapsed:: true`.
    pub collapsed: bool,
    /// 1 = top level.
    pub depth: u32,
    /// Opaque content version of the block.
    pub version: String,
    /// Nested blocks (tree tools).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub children: Vec<BlockInfo>,
    /// The page-properties pre-block.
    #[serde(skip)]
    #[schemars(skip)]
    pub is_pre_block: bool,
}

/// Which hit kinds `search` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    /// Pages only.
    Page,
    /// Blocks only.
    Block,
    /// Both.
    #[default]
    All,
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchItem {
    /// `page` or `block`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Block UUID (blocks only).
    pub uuid: Option<String>,
    /// Display name of the page (the page itself for page hits).
    pub page: String,
    /// Text window around the match.
    pub snippet: String,
    /// Higher is better.
    pub score: f64,
    /// Ancestor titles, outermost first (blocks only).
    pub breadcrumb: Vec<String>,
}

/// Parameters of [`GraphReader::search`].
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// User query text.
    pub query: String,
    /// Maximum hits.
    pub limit: usize,
    /// Hit kinds.
    pub kind: SearchKind,
    /// Restrict to the blocks of this page.
    pub page: Option<String>,
}

/// Parameters of [`GraphReader::list_pages`].
#[derive(Debug, Clone, Default)]
pub struct ListPagesQuery {
    /// Only descendants of this namespace.
    pub namespace: Option<String>,
    /// Only pages tagged with this page.
    pub tag: Option<String>,
    /// Only pages updated at or after this unix-ms time.
    pub modified_since: Option<i64>,
    /// Include journal pages.
    pub journals: bool,
    /// Skip this many results.
    pub offset: usize,
    /// Maximum results.
    pub limit: usize,
}

/// A reference hit with its breadcrumb.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RefItem {
    /// The referencing block.
    pub block: BlockInfo,
    /// Ancestor titles, outermost first.
    pub breadcrumb: Vec<String>,
}

/// References grouped by page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RefGroupInfo {
    /// Display name of the referencing page.
    pub page: String,
    /// Referencing blocks.
    pub blocks: Vec<RefItem>,
}

/// Task filter of [`GraphReader::tasks`].
#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    /// Markers (`TODO`, `DOING`, ...); empty = all.
    pub markers: Vec<String>,
    /// Priority `A`/`B`/`C`.
    pub priority: Option<String>,
    /// Page name.
    pub page: Option<String>,
}

/// Something that changed in the graph; drives resource notifications.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeEvent {
    /// Display names of pages whose content or references changed.
    pub pages: Vec<String>,
    /// `yyyy-mm-dd` of journal pages among `pages`.
    pub journal_days: Vec<String>,
    /// Block UUIDs that appeared or disappeared.
    pub blocks: Vec<String>,
    /// Everything may have changed (bulk reindex).
    pub all: bool,
}

/// Synchronous read access to the graph. Called from `spawn_blocking`, so implementations may block.
pub trait GraphReader: Send + Sync + 'static {
    /// Describe the active graph.
    fn graph_info(&self) -> ReaderResult<GraphInfo>;

    /// Full-text search.
    fn search(&self, _q: &SearchQuery) -> ReaderResult<Vec<SearchItem>> {
        Err(ReaderError::unsupported("search"))
    }
    /// A page by name or alias (case-insensitive).
    fn page(&self, _name: &str) -> ReaderResult<Option<PageInfo>> {
        Err(ReaderError::unsupported("page"))
    }
    /// Pages sorted by name.
    fn list_pages(&self, _q: &ListPagesQuery) -> ReaderResult<Vec<PageInfo>> {
        Err(ReaderError::unsupported("list_pages"))
    }
    /// Most recently updated file-backed pages.
    fn recent_pages(&self, _offset: usize, _limit: usize) -> ReaderResult<Vec<PageInfo>> {
        Err(ReaderError::unsupported("recent_pages"))
    }
    /// Flat pre-order blocks of a page, `limit` blocks from `offset` (pre-block first).
    fn page_blocks(
        &self,
        _page: &PageInfo,
        _offset: usize,
        _limit: usize,
        _skip_collapsed: bool,
    ) -> ReaderResult<Vec<BlockInfo>> {
        Err(ReaderError::unsupported("page_blocks"))
    }
    /// One block.
    fn block(&self, _uuid: &str) -> ReaderResult<Option<BlockInfo>> {
        Err(ReaderError::unsupported("block"))
    }
    /// A block and its descendants, flat pre-order.
    fn subtree(&self, _uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        Err(ReaderError::unsupported("subtree"))
    }
    /// Ancestors of a block, outermost first.
    fn ancestors(&self, _uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        Err(ReaderError::unsupported("ancestors"))
    }
    /// Journal pages, newest first, strictly before `before_day` (`yyyyMMdd`).
    fn journals(&self, _before_day: Option<i64>, _limit: usize) -> ReaderResult<Vec<PageInfo>> {
        Err(ReaderError::unsupported("journals"))
    }
    /// The journal page of a day (`yyyyMMdd`).
    fn journal(&self, _day: i64) -> ReaderResult<Option<PageInfo>> {
        Err(ReaderError::unsupported("journal"))
    }
    /// Today as `yyyyMMdd` in the local time zone.
    fn today(&self) -> i64 {
        crate::dates::today_int()
    }
    /// Linked references of a page, grouped by page.
    fn linked_references(&self, _page: &PageInfo) -> ReaderResult<Vec<RefGroupInfo>> {
        Err(ReaderError::unsupported("linked_references"))
    }
    /// Unlinked (plain text) mentions of a page.
    fn unlinked_references(&self, _page: &PageInfo) -> ReaderResult<Vec<RefGroupInfo>> {
        Err(ReaderError::unsupported("unlinked_references"))
    }
    /// Blocks that reference a block by `((uuid))`.
    fn block_referrers(&self, _uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        Err(ReaderError::unsupported("block_referrers"))
    }
    /// Task blocks.
    fn tasks(&self, _q: &TaskQuery) -> ReaderResult<Vec<BlockInfo>> {
        Err(ReaderError::unsupported("tasks"))
    }
    /// Raw text of the page's file.
    fn page_file_text(&self, _page: &PageInfo) -> ReaderResult<Option<String>> {
        Err(ReaderError::unsupported("page_file_text"))
    }
    /// Raw text of `logseq/config.edn`.
    fn config_text(&self) -> ReaderResult<Option<String>> {
        Err(ReaderError::unsupported("config_text"))
    }
    /// Bytes of a file under `assets/` (path relative to `assets/`); `None` when missing.
    /// Implementations must confine the path to `assets/` and refuse files above `max_bytes`.
    fn read_asset(&self, _rel: &str, _max_bytes: u64) -> ReaderResult<Option<Vec<u8>>> {
        Err(ReaderError::unsupported("read_asset"))
    }
    /// A receiver of graph changes, if this reader can observe them.
    fn changes(&self) -> Option<broadcast::Receiver<ChangeEvent>> {
        None
    }
}

/// Minimal reader that reports a fixed name/path; used where no index is available.
#[derive(Debug, Clone)]
pub struct StaticGraphReader {
    info: GraphInfo,
}

impl StaticGraphReader {
    /// Create a reader with unknown counts.
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            info: GraphInfo {
                name: name.into(),
                path: path.into(),
                page_count: None,
                block_count: None,
            },
        }
    }
}

impl GraphReader for StaticGraphReader {
    fn graph_info(&self) -> ReaderResult<GraphInfo> {
        Ok(self.info.clone())
    }
}
