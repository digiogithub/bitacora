//! Typed read API over the index (design §5): page outlines, subtrees, breadcrumbs, references,
//! tasks, namespaces and graph edges. No `rusqlite` type crosses this boundary.
//!
//! [`IndexReader`] wraps a [`ReaderPool`]; every method checks out a read-only connection for the
//! duration of the call.

mod diagnostics;
mod misc;
mod outline;
mod refs;
mod unlinked;

use rusqlite::{Connection, Row};

use crate::Error;
use crate::pool::ReaderPool;

pub use diagnostics::{DiagnosticCount, DiagnosticFilter, DiagnosticRow};
pub use misc::{
    AgendaItem, AgendaKind, GraphEdge, GraphEdgeKind, GraphNode, GraphOptions, GraphView,
    NamespaceNode, TaskFilter, TaskItem,
};
pub use outline::{PageFilter, PageSort};
pub use refs::{Crumb, RefFilters, RefGroup, RefHit};

/// SQL used by the read API that tests assert query plans on.
#[doc(hidden)]
pub mod sql {
    pub use super::outline::{OUTLINE_SQL, SUBTREE_SQL};
    pub use super::unlinked::UNLINKED_FTS_SQL;
}

/// A page (file-backed or placeholder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRow {
    /// Internal id (stable across reparses of the page's file).
    pub id: i64,
    /// Normalised name (`pages.name`).
    pub name: String,
    /// Display name.
    pub original_name: String,
    /// Page UUID (v5 of the name).
    pub uuid: String,
    /// Relative path of the defining file; `None` for placeholders.
    pub file_path: Option<String>,
    /// Journal page.
    pub is_journal: bool,
    /// `yyyyMMdd` of a journal page.
    pub journal_day: Option<i64>,
    /// Namespace parent page id.
    pub namespace_parent_id: Option<i64>,
    /// Built-in page (`TODO`, `A`, ...).
    pub is_builtin: bool,
    /// Creation time (unix ms).
    pub created_at: Option<i64>,
    /// Update time (unix ms).
    pub updated_at: Option<i64>,
}

impl PageRow {
    /// A page with no defining file.
    #[must_use]
    pub fn is_placeholder(&self) -> bool {
        self.file_path.is_none()
    }
}

/// A block with the fields views need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRow {
    /// Internal id (reassigned when the block is replaced).
    pub id: i64,
    /// Block UUID.
    pub uuid: String,
    /// Owning page id.
    pub page_id: i64,
    /// Owning file id.
    pub file_id: i64,
    /// Parent block id (`None` = top level).
    pub parent_id: Option<i64>,
    /// Pre-order position in the file.
    pub ord: i64,
    /// Highest `ord` in the block's subtree.
    pub subtree_end: i64,
    /// 1 = top level.
    pub depth: i64,
    /// The page-properties pre-block.
    pub is_pre_block: bool,
    /// Raw content.
    pub content: String,
    /// First line without marker/priority.
    pub title: String,
    /// Task marker.
    pub marker: Option<String>,
    /// Priority `A`/`B`/`C`.
    pub priority: Option<String>,
    /// `yyyyMMdd`.
    pub scheduled: Option<i64>,
    /// `yyyyMMdd`.
    pub deadline: Option<i64>,
    /// `collapsed:: true`.
    pub collapsed: bool,
    /// Markdown heading size.
    pub heading: Option<i64>,
    /// Start of the byte span in the file.
    pub byte_start: i64,
    /// End of the byte span.
    pub byte_end: i64,
    /// 1-based first line.
    pub line_start: i64,
    /// `(raw_key, raw_value)` in file order.
    pub properties: Vec<(String, String)>,
}

pub(crate) const BLOCK_COLS: &str = "b.id, b.uuid, b.page_id, b.file_id, b.parent_id, b.ord, \
    b.subtree_end, b.depth, b.is_pre_block, b.content, b.title, b.marker, b.priority, \
    b.scheduled, b.deadline, b.collapsed, b.heading, b.byte_start, b.byte_end, b.line_start";

pub(crate) const PAGE_COLS: &str = "p.id, p.name, p.original_name, p.uuid, \
    (SELECT f.path FROM files f WHERE f.id = p.file_id), p.is_journal, p.journal_day, \
    p.namespace_parent_id, p.is_builtin, p.created_at, p.updated_at";

pub(crate) fn block_from_row(r: &Row<'_>) -> rusqlite::Result<BlockRow> {
    Ok(BlockRow {
        id: r.get(0)?,
        uuid: r.get(1)?,
        page_id: r.get(2)?,
        file_id: r.get(3)?,
        parent_id: r.get(4)?,
        ord: r.get(5)?,
        subtree_end: r.get(6)?,
        depth: r.get(7)?,
        is_pre_block: r.get(8)?,
        content: r.get(9)?,
        title: r.get(10)?,
        marker: r.get(11)?,
        priority: r.get(12)?,
        scheduled: r.get(13)?,
        deadline: r.get(14)?,
        collapsed: r.get(15)?,
        heading: r.get(16)?,
        byte_start: r.get(17)?,
        byte_end: r.get(18)?,
        line_start: r.get(19)?,
        properties: Vec::new(),
    })
}

pub(crate) fn page_from_row(r: &Row<'_>) -> rusqlite::Result<PageRow> {
    Ok(PageRow {
        id: r.get(0)?,
        name: r.get(1)?,
        original_name: r.get(2)?,
        uuid: r.get(3)?,
        file_path: r.get(4)?,
        is_journal: r.get(5)?,
        journal_day: r.get(6)?,
        namespace_parent_id: r.get(7)?,
        is_builtin: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

/// Fill `properties` of each block (one cached statement, one lookup per block).
pub(crate) fn load_properties(conn: &Connection, blocks: &mut [BlockRow]) -> Result<(), Error> {
    let mut st = conn.prepare_cached(
        "SELECT raw_key, raw_value FROM block_properties WHERE block_id = ?1 ORDER BY pos",
    )?;
    for b in blocks {
        b.properties = st
            .query_map([b.id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
    }
    Ok(())
}

/// Read API handle. Cheap to clone; shares the reader pool.
#[derive(Debug, Clone)]
pub struct IndexReader {
    pool: ReaderPool,
}

impl IndexReader {
    /// Wrap a reader pool.
    #[must_use]
    pub fn new(pool: ReaderPool) -> Self {
        Self { pool }
    }

    pub(crate) fn conn(&self) -> Result<crate::pool::PooledReader, Error> {
        self.pool.get()
    }
}

impl crate::Index {
    /// A typed read API over this index's reader pool.
    #[must_use]
    pub fn read_api(&self) -> IndexReader {
        IndexReader::new(self.readers().clone())
    }
}
