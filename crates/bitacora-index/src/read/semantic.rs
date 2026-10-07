//! Block export for the semantic (Pando) indexer (BIT-US-0142): everything a block needs to
//! become a knowledge-base document, read in one pass per file.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};

use super::{IndexReader, load_properties_for};
use crate::Error;

/// A block with its page context, as the semantic indexer needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticBlock {
    /// Block UUID (`id::` when present, else the index UUID).
    pub uuid: String,
    /// Graph-relative path of the file holding the block.
    pub file_path: String,
    /// Display name of the page.
    pub page_title: String,
    /// Journal page.
    pub is_journal: bool,
    /// `yyyyMMdd` of a journal page.
    pub journal_day: Option<i64>,
    /// Titles of the ancestors, outermost first (page-properties pre-block excluded).
    pub breadcrumb: Vec<String>,
    /// Raw block content.
    pub content: String,
    /// Task marker.
    pub marker: Option<String>,
    /// Depth (1 = top level).
    pub depth: i64,
    /// The page-properties pre-block.
    pub is_pre_block: bool,
    /// Tags of the page (`tags::`) followed by the `#tags` of the block, de-duplicated.
    pub tags: Vec<String>,
    /// `(raw_key, raw_value)` properties in file order.
    pub properties: Vec<(String, String)>,
    /// Properties of the page (those of its page-properties pre-block), in file order.
    pub page_properties: Vec<(String, String)>,
    /// Properties of every ancestor block (page-properties pre-block excluded), so a privacy
    /// property on a parent covers its subtree.
    pub ancestor_properties: Vec<(String, String)>,
}

struct Raw {
    id: i64,
    uuid: String,
    parent: Option<i64>,
    depth: i64,
    pre: bool,
    content: String,
    title: String,
    marker: Option<String>,
    page_id: i64,
    page_title: String,
    is_journal: bool,
    journal_day: Option<i64>,
}

impl IndexReader {
    /// Graph-relative paths of every page and journal file that parsed (`status = 'ok'`).
    pub fn semantic_file_paths(&self) -> Result<Vec<String>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(
            "SELECT path FROM files WHERE kind IN ('page','journal') AND status = 'ok' \
             ORDER BY path",
        )?;
        Ok(st
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?)
    }

    /// Every block of the file at `path`, in outline order; empty when the file is unknown.
    pub fn semantic_blocks(&self, path: &str) -> Result<Vec<SemanticBlock>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(
            "SELECT b.id, b.uuid, b.parent_id, b.depth, b.is_pre_block, b.content, b.title, \
                    b.marker, p.id, p.original_name, p.is_journal, p.journal_day \
             FROM blocks b JOIN files f ON f.id = b.file_id JOIN pages p ON p.id = b.page_id \
             WHERE f.path = ?1 ORDER BY b.ord",
        )?;
        let raws: Vec<Raw> = st
            .query_map([path], |r| {
                Ok(Raw {
                    id: r.get(0)?,
                    uuid: r.get(1)?,
                    parent: r.get(2)?,
                    depth: r.get(3)?,
                    pre: r.get(4)?,
                    content: r.get(5)?,
                    title: r.get(6)?,
                    marker: r.get(7)?,
                    page_id: r.get(8)?,
                    page_title: r.get(9)?,
                    is_journal: r.get(10)?,
                    journal_day: r.get(11)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        drop(st);
        let Some(first) = raws.first() else {
            return Ok(Vec::new());
        };
        let page_tags: Vec<String> = {
            let mut st = conn.prepare_cached(
                "SELECT t.original_name FROM page_tags pt JOIN pages t ON t.id = pt.tag_page_id \
                 WHERE pt.page_id = ?1 ORDER BY t.original_name",
            )?;
            st.query_map([first.page_id], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        let mut block_tags: HashMap<i64, Vec<String>> = HashMap::new();
        {
            let mut st = conn.prepare_cached(
                "SELECT r.block_id, t.original_name FROM block_page_refs r \
                 JOIN blocks b ON b.id = r.block_id JOIN files f ON f.id = b.file_id \
                 JOIN pages t ON t.id = r.page_id \
                 WHERE f.path = ?1 AND r.kind = 2 ORDER BY r.block_id, t.original_name",
            )?;
            let rows = st.query_map([path], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, name) = row?;
                block_tags.entry(id).or_default().push(name);
            }
        }
        let by_id: HashMap<i64, usize> = raws.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        let ids: Vec<i64> = raws.iter().map(|r| r.id).collect();
        let props = load_properties_for(&conn, &ids)?;
        let props_by_idx: Vec<Vec<(String, String)>> = props.clone();
        let page_properties = raws
            .iter()
            .zip(&props)
            .find(|(r, _)| r.pre)
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        let mut out = Vec::with_capacity(raws.len());
        for (r, properties) in raws.iter().zip(props) {
            let mut crumbs = Vec::new();
            let mut ancestor_properties: Vec<(String, String)> = Vec::new();
            let mut cur = r.parent;
            // Bounded by the number of blocks: a corrupt parent cycle cannot loop forever.
            for _ in 0..raws.len() {
                let Some(i) = cur.and_then(|p| by_id.get(&p)).copied() else {
                    break;
                };
                let anc = &raws[i];
                if !anc.pre {
                    crumbs.push(anc.title.clone());
                    ancestor_properties.extend(props_by_idx[i].iter().cloned());
                }
                cur = anc.parent;
            }
            crumbs.reverse();
            let mut tags = page_tags.clone();
            for t in block_tags.get(&r.id).into_iter().flatten() {
                if !tags.contains(t) {
                    tags.push(t.clone());
                }
            }
            out.push(SemanticBlock {
                uuid: r.uuid.clone(),
                file_path: path.to_owned(),
                page_title: r.page_title.clone(),
                is_journal: r.is_journal,
                journal_day: r.journal_day,
                breadcrumb: crumbs,
                content: r.content.clone(),
                marker: r.marker.clone(),
                depth: r.depth,
                is_pre_block: r.pre,
                tags,
                properties,
                page_properties: page_properties.clone(),
                ancestor_properties,
            });
        }
        Ok(out)
    }

    /// One block with its page context, or `None` when the UUID is unknown.
    pub fn semantic_block(&self, uuid: &str) -> Result<Option<SemanticBlock>, Error> {
        let path: Option<String> = {
            let conn = self.conn()?;
            conn.query_row(
                "SELECT f.path FROM blocks b JOIN files f ON f.id = b.file_id WHERE b.uuid = ?1",
                params![uuid.to_ascii_lowercase()],
                |r| r.get(0),
            )
            .optional()?
        };
        let Some(path) = path else { return Ok(None) };
        Ok(self
            .semantic_blocks(&path)?
            .into_iter()
            .find(|b| b.uuid.eq_ignore_ascii_case(uuid)))
    }
}
