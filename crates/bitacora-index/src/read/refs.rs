//! Alias closure and linked references (BIT-SP-0003.R9).
//!
//! Logseq `:block/refs` is a kind-agnostic page set: a page referenced as `[[x]]`, `#x` and
//! `#[[x]]` in one block is one reference. Every query here therefore matches on `page_id`
//! regardless of `block_page_refs.kind`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use bitacora_core::naming::page_key;
use regex::Regex;
use rusqlite::{OptionalExtension, params};

use super::outline::with_cols;
use super::{
    BlockRow, IndexReader, PAGE_COLS, PageRow, block_from_row, load_properties, page_from_row,
};
use crate::Error;

/// An ancestor in a breadcrumb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    /// Ancestor block UUID.
    pub uuid: String,
    /// Display title of the ancestor.
    pub title: String,
}

/// One matching block with its ancestors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefHit {
    /// The referencing block (its subtree is reachable through [`IndexReader::subtree`]).
    pub block: BlockRow,
    /// Ancestors, outermost first.
    pub breadcrumb: Vec<Crumb>,
}

/// References grouped by the page they live on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefGroup {
    /// The referencing page.
    pub page: PageRow,
    /// Matching blocks in outline order; nested matches are folded into their top-most ancestor.
    pub blocks: Vec<RefHit>,
}

/// `filters::` include/exclude sets, by page title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefFilters {
    /// Every one of these pages must be in the block's path-refs.
    pub include: Vec<String>,
    /// None of these pages may be in the block's path-refs.
    pub exclude: Vec<String>,
}

impl RefFilters {
    /// Parse the raw value of a `filters::` page property: a map of page title to boolean,
    /// `{"a" true, "b" false}` (`true` includes, `false` excludes).
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        static RE: OnceLock<Option<Regex>> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r#""((?:[^"\\]|\\.)*)"\s*(true|false)"#).ok());
        let mut f = Self::default();
        let Some(re) = re else { return f };
        for c in re.captures_iter(raw) {
            let name = c[1].replace("\\\"", "\"").replace("\\\\", "\\");
            if &c[2] == "true" {
                f.include.push(name);
            } else {
                f.exclude.push(name);
            }
        }
        f
    }

    /// No filter at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}

const CLOSURE_CTE: &str = "WITH RECURSIVE al(id, hops) AS ( \
    SELECT ?1, 0 \
    UNION \
    SELECT CASE WHEN a.page_id = al.id THEN a.alias_page_id ELSE a.page_id END, al.hops + 1 \
    FROM page_aliases a JOIN al ON al.id IN (a.page_id, a.alias_page_id) \
    WHERE al.hops < 2)";

/// SQL fragment: is page `{ID}` in the path-refs of block `b`.
fn in_path_refs(id: i64) -> String {
    format!(
        "(b.page_id = {id} OR EXISTS (SELECT 1 FROM blocks a \
         JOIN block_page_refs x ON x.block_id = a.id \
         WHERE a.file_id = b.file_id AND a.ord <= b.ord AND a.subtree_end >= b.ord \
         AND x.page_id = {id}))"
    )
}

impl IndexReader {
    /// Symmetric 2-hop alias closure of a page (Logseq's `alias` rule), including the page itself.
    pub fn alias_closure(&self, page_id: i64) -> Result<Vec<i64>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&format!(
            "{CLOSURE_CTE} SELECT DISTINCT id FROM al ORDER BY id"
        ))?;
        let ids = st
            .query_map([page_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// For an alias page without content of its own, the page that declares it as an alias.
    pub fn alias_redirect(&self, page_id: i64) -> Result<Option<PageRow>, Error> {
        let conn = self.conn()?;
        let content: i64 = conn.query_row(
            "SELECT count(*) FROM blocks WHERE page_id = ?1 AND is_pre_block = 0 \
             AND trim(content) <> ''",
            [page_id],
            |r| r.get(0),
        )?;
        if content > 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!(
                    "SELECT {PAGE_COLS} FROM page_aliases al JOIN pages p ON p.id = al.page_id \
                     WHERE al.alias_page_id = ?1 ORDER BY p.name LIMIT 1"
                ),
                [page_id],
                page_from_row,
            )
            .optional()?)
    }

    /// The `filters::` property of a page, parsed.
    pub fn page_ref_filters(&self, page_id: i64) -> Result<RefFilters, Error> {
        let conn = self.conn()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT raw_value FROM page_properties WHERE page_id = ?1 AND key = 'filters'",
                [page_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map_or_else(RefFilters::default, |r| RefFilters::parse(&r)))
    }

    /// Linked references of a page, honouring its own `filters::` property.
    pub fn linked_references(&self, page_id: i64) -> Result<Vec<RefGroup>, Error> {
        let filters = self.page_ref_filters(page_id)?;
        self.linked_references_with(page_id, &filters)
    }

    /// Linked references with explicit filters: blocks whose own refs hit the alias closure of
    /// the page, outside the page itself, whose path-refs contain every `include` page and no
    /// `exclude` page. Groups: journals newest first, then pages by name.
    pub fn linked_references_with(
        &self,
        page_id: i64,
        filters: &RefFilters,
    ) -> Result<Vec<RefGroup>, Error> {
        let conn = self.conn()?;
        let ids_of = |names: &[String]| -> Result<Vec<i64>, Error> {
            let mut out = Vec::new();
            for n in names {
                let id: Option<i64> = conn
                    .query_row("SELECT id FROM pages WHERE name = ?1", [page_key(n)], |r| {
                        r.get(0)
                    })
                    .optional()?;
                out.extend(id);
            }
            Ok(out)
        };
        let mut sql = format!(
            "{CLOSURE_CTE} SELECT DISTINCT {cols} FROM block_page_refs r \
             JOIN blocks b ON b.id = r.block_id \
             WHERE r.page_id IN (SELECT id FROM al) AND b.page_id <> ?1",
            cols = super::BLOCK_COLS
        );
        for id in ids_of(&filters.include)? {
            let _ = write!(sql, " AND {}", in_path_refs(id));
        }
        for id in ids_of(&filters.exclude)? {
            let _ = write!(sql, " AND NOT {}", in_path_refs(id));
        }
        sql.push_str(" ORDER BY b.page_id, b.file_id, b.ord");
        let mut st = conn.prepare(&sql)?;
        let rows: Vec<BlockRow> = st
            .query_map(params![page_id], block_from_row)?
            .collect::<Result<_, _>>()?;
        drop(st);
        group_hits(&conn, rows, true)
    }
}

/// Keep the top-most block of each nested run, attach breadcrumbs and group by page.
pub(crate) fn group_hits(
    conn: &rusqlite::Connection,
    rows: Vec<BlockRow>,
    fold_nested: bool,
) -> Result<Vec<RefGroup>, Error> {
    let mut kept: Vec<BlockRow> = Vec::new();
    for b in rows {
        let nested = fold_nested
            && kept
                .last()
                .is_some_and(|k| k.file_id == b.file_id && b.ord <= k.subtree_end);
        if !nested {
            kept.push(b);
        }
    }
    load_properties(conn, &mut kept)?;
    let mut crumbs = conn.prepare_cached(&with_cols(
        "SELECT {COLS} FROM blocks c JOIN blocks b \
         ON b.file_id = c.file_id AND b.ord < c.ord AND b.subtree_end >= c.ord \
         WHERE c.id = ?1 AND b.is_pre_block = 0 ORDER BY b.ord",
    ))?;
    let mut by_page: HashMap<i64, Vec<RefHit>> = HashMap::new();
    for block in kept {
        let breadcrumb = crumbs
            .query_map([block.id], block_from_row)?
            .map(|r| {
                r.map(|a| Crumb {
                    uuid: a.uuid,
                    title: a.title,
                })
            })
            .collect::<Result<_, _>>()?;
        by_page
            .entry(block.page_id)
            .or_default()
            .push(RefHit { block, breadcrumb });
    }
    let mut page_st =
        conn.prepare_cached(&format!("SELECT {PAGE_COLS} FROM pages p WHERE p.id = ?1"))?;
    let mut groups = Vec::new();
    for (pid, blocks) in by_page {
        let page = page_st.query_row([pid], page_from_row)?;
        groups.push(RefGroup { page, blocks });
    }
    groups.sort_by(|a, b| {
        b.page
            .is_journal
            .cmp(&a.page.is_journal)
            .then_with(|| b.page.journal_day.cmp(&a.page.journal_day))
            .then_with(|| a.page.name.cmp(&b.page.name))
    });
    Ok(groups)
}
