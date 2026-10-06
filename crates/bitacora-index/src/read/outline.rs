//! Page lookups, outline, subtree and ancestors (BIT-SP-0003.R4).

use bitacora_core::naming::page_key;
use rusqlite::{OptionalExtension, params};

use super::{
    BLOCK_COLS, BlockRow, IndexReader, PAGE_COLS, PageRow, block_from_row, load_properties,
    page_from_row,
};
use crate::Error;

/// Visible outline: blocks of a page in file order, optionally skipping collapsed subtrees.
/// Interval predicate on `(file_id, ord)`; uses the `blocks_page` index.
pub const OUTLINE_SQL: &str = "SELECT {COLS} FROM blocks b \
    WHERE b.page_id = ?1 \
      AND (?4 = 0 OR NOT EXISTS (SELECT 1 FROM blocks c \
                  WHERE c.file_id = b.file_id AND c.collapsed = 1 \
                    AND b.ord > c.ord AND b.ord <= c.subtree_end)) \
    ORDER BY b.file_id, b.ord LIMIT ?2 OFFSET ?3";

/// Block and its descendants, by interval join on `UNIQUE(file_id, ord)`.
pub const SUBTREE_SQL: &str = "SELECT {COLS} FROM blocks a JOIN blocks b \
    ON b.file_id = a.file_id AND b.ord BETWEEN a.ord AND a.subtree_end \
    WHERE a.uuid = ?1 ORDER BY b.ord";

pub(crate) fn with_cols(sql: &str) -> String {
    sql.replace("{COLS}", BLOCK_COLS)
}

/// Which pages [`IndexReader::all_pages`] lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFilter {
    /// Include journal pages.
    pub journals: bool,
    /// Include placeholder pages (referenced but without a file).
    pub placeholders: bool,
    /// Include built-in pages.
    pub builtins: bool,
}

impl Default for PageFilter {
    fn default() -> Self {
        Self {
            journals: true,
            placeholders: false,
            builtins: false,
        }
    }
}

/// Order of [`IndexReader::all_pages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSort {
    /// By normalised name.
    #[default]
    Name,
    /// Most recently updated first.
    Updated,
    /// Most recently created first.
    Created,
}

impl IndexReader {
    /// Look a page up by title (normalised like Logseq: case-insensitive, NFC).
    pub fn page_by_name(&self, name: &str) -> Result<Option<PageRow>, Error> {
        let key = page_key(name);
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {PAGE_COLS} FROM pages p WHERE p.name = ?1"),
                [key],
                page_from_row,
            )
            .optional()?)
    }

    /// Look a page up by its UUID.
    pub fn page_by_uuid(&self, uuid: &str) -> Result<Option<PageRow>, Error> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {PAGE_COLS} FROM pages p WHERE p.uuid = ?1"),
                [uuid.to_ascii_lowercase()],
                page_from_row,
            )
            .optional()?)
    }

    /// Look a page up by internal id.
    pub fn page_by_id(&self, id: i64) -> Result<Option<PageRow>, Error> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {PAGE_COLS} FROM pages p WHERE p.id = ?1"),
                [id],
                page_from_row,
            )
            .optional()?)
    }

    /// List pages.
    pub fn all_pages(&self, filter: PageFilter, sort: PageSort) -> Result<Vec<PageRow>, Error> {
        let order = match sort {
            PageSort::Name => "p.name",
            PageSort::Updated => "p.updated_at DESC, p.name",
            PageSort::Created => "p.created_at DESC, p.name",
        };
        let sql = format!(
            "SELECT {PAGE_COLS} FROM pages p WHERE (?1 = 1 OR p.is_journal = 0) \
             AND (?2 = 1 OR p.file_id IS NOT NULL) AND (?3 = 1 OR p.is_builtin = 0) ORDER BY {order}"
        );
        let conn = self.conn()?;
        let mut st = conn.prepare(&sql)?;
        let rows = st
            .query_map(
                params![filter.journals, filter.placeholders, filter.builtins],
                page_from_row,
            )?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Journal pages, newest first, strictly before `before_day` (`yyyyMMdd`) when given.
    pub fn journals(&self, before_day: Option<i64>, limit: usize) -> Result<Vec<PageRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare(&format!(
            "SELECT {PAGE_COLS} FROM pages p WHERE p.is_journal = 1 AND p.file_id IS NOT NULL \
             AND (?1 IS NULL OR p.journal_day < ?1) ORDER BY p.journal_day DESC LIMIT ?2"
        ))?;
        let rows = st
            .query_map(params![before_day, limit as i64], page_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// `limit` blocks of the page starting at the `offset`-th visible block (file order). With
    /// `skip_collapsed` descendants of collapsed blocks are not counted nor returned. The
    /// page-properties pre-block (if any) comes first and is flagged `is_pre_block`.
    pub fn outline(
        &self,
        page_id: i64,
        offset: usize,
        limit: usize,
        skip_collapsed: bool,
    ) -> Result<Vec<BlockRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&with_cols(OUTLINE_SQL))?;
        let mut rows: Vec<BlockRow> = st
            .query_map(
                params![page_id, limit as i64, offset as i64, skip_collapsed],
                block_from_row,
            )?
            .collect::<Result<_, _>>()?;
        drop(st);
        load_properties(&conn, &mut rows)?;
        Ok(rows)
    }

    /// The block and all its descendants in outline order.
    pub fn subtree(&self, uuid: &str) -> Result<Vec<BlockRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&with_cols(SUBTREE_SQL))?;
        let mut rows: Vec<BlockRow> = st
            .query_map([uuid.to_ascii_lowercase()], block_from_row)?
            .collect::<Result<_, _>>()?;
        drop(st);
        load_properties(&conn, &mut rows)?;
        Ok(rows)
    }

    /// Ancestors of the block, outermost first (breadcrumb); empty for a top-level block.
    pub fn ancestors(&self, uuid: &str) -> Result<Vec<BlockRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&with_cols(
            "SELECT {COLS} FROM blocks c JOIN blocks b \
             ON b.file_id = c.file_id AND b.ord < c.ord AND b.subtree_end >= c.ord \
             WHERE c.uuid = ?1 AND b.is_pre_block = 0 ORDER BY b.ord",
        ))?;
        let mut rows: Vec<BlockRow> = st
            .query_map([uuid.to_ascii_lowercase()], block_from_row)?
            .collect::<Result<_, _>>()?;
        drop(st);
        load_properties(&conn, &mut rows)?;
        Ok(rows)
    }

    /// One block by UUID.
    pub fn block(&self, uuid: &str) -> Result<Option<BlockRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&format!(
            "SELECT {BLOCK_COLS} FROM blocks b WHERE b.uuid = ?1"
        ))?;
        let found = st
            .query_row([uuid.to_ascii_lowercase()], block_from_row)
            .optional()?;
        drop(st);
        let mut v: Vec<BlockRow> = found.into_iter().collect();
        load_properties(&conn, &mut v)?;
        Ok(v.pop())
    }
}
