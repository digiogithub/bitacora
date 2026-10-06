//! [`GraphReader`] over the SQLite index (`bitacora-index`) and the graph folder.
//!
//! Page text, config and assets are read from the graph folder (never written); everything else
//! comes from [`IndexReader`] and [`bitacora_index::search::search`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::Receiver;

use bitacora_index::search::{Scope, SearchHit, SearchOptions, search};
use bitacora_index::{
    BlockRow, Index, IndexEvent, IndexReader, IndexStats, PageFilter, PageRow, PageSort,
    ReaderPool, RefGroup, TaskFilter,
};
use tokio::sync::broadcast;

use crate::dates;
use crate::reader::{
    BlockInfo, ChangeEvent, GraphInfo, GraphReader, ListPagesQuery, PageInfo, ReaderError,
    ReaderResult, RefGroupInfo, RefItem, SearchItem, SearchKind, SearchQuery, TaskQuery,
};

fn ie(e: impl std::fmt::Display) -> ReaderError {
    ReaderError::internal(e.to_string())
}

/// Reader backed by the index of one graph.
#[derive(Debug, Clone)]
pub struct IndexGraphReader {
    api: IndexReader,
    pool: ReaderPool,
    root: PathBuf,
    name: String,
    changes: broadcast::Sender<ChangeEvent>,
}

impl IndexGraphReader {
    /// Wrap an open index. `graph_root` is the graph folder (page text, config and assets).
    pub fn new(index: &Index, graph_root: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        let (changes, _) = broadcast::channel(256);
        Self {
            api: index.read_api(),
            pool: index.readers().clone(),
            root: graph_root.into(),
            name: name.into(),
            changes,
        }
    }

    /// Forward index events (from `Indexer::subscribe`) as [`ChangeEvent`]s on a background thread.
    /// The thread ends when the sender side of `events` is dropped.
    pub fn forward_events(&self, events: Receiver<IndexEvent>) {
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("bitacora-mcp-events".into())
            .spawn(move || {
                while let Ok(ev) = events.recv() {
                    if let Some(change) = this.change_of(&ev) {
                        // No subscriber is fine.
                        let _ = this.changes.send(change);
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!(error = %e, "cannot start the MCP change forwarder");
        }
    }

    fn change_of(&self, ev: &IndexEvent) -> Option<ChangeEvent> {
        let (ids, added, removed): (&[i64], &[String], &[String]) = match ev {
            IndexEvent::FileReplaced {
                page_ids_touched,
                block_uuids_added,
                block_uuids_removed,
                ..
            } => (page_ids_touched, block_uuids_added, block_uuids_removed),
            IndexEvent::FileDeleted {
                page_ids_touched,
                block_uuids_removed,
                ..
            } => (page_ids_touched, &[], block_uuids_removed),
            IndexEvent::FileRenamed { .. } => return None,
            IndexEvent::BulkFinished => {
                return Some(ChangeEvent {
                    all: true,
                    ..ChangeEvent::default()
                });
            }
        };
        let mut change = ChangeEvent::default();
        for id in ids {
            if let Ok(Some(p)) = self.api.page_by_id(*id) {
                if let Some(d) = p.journal_day {
                    change.journal_days.push(dates::to_iso(d));
                }
                change.pages.push(p.original_name);
            }
        }
        change.blocks.extend(added.iter().cloned());
        change.blocks.extend(removed.iter().cloned());
        Some(change)
    }

    fn page_info(&self, row: &PageRow) -> ReaderResult<PageInfo> {
        let mut properties = BTreeMap::new();
        let first = self.api.outline(row.id, 0, 1, false).map_err(ie)?;
        if let Some(pre) = first.first().filter(|b| b.is_pre_block) {
            for (k, v) in &pre.properties {
                properties.insert(k.clone(), v.clone());
            }
        }
        let prop = |key: &str| -> Vec<String> {
            properties
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| split_page_list(v))
                .unwrap_or_default()
        };
        let (aliases, tags) = (prop("alias"), prop("tags"));
        let conn = self.pool.get().map_err(ie)?;
        let block_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM blocks WHERE page_id = ?1 AND is_pre_block = 0",
                [row.id],
                |r| r.get(0),
            )
            .map_err(ie)?;
        let etag = match &row.file_path {
            Some(path) => conn
                .query_row(
                    "SELECT hex(content_hash) FROM files WHERE path = ?1",
                    [path],
                    |r| r.get::<_, String>(0),
                )
                .map(|h| h.to_ascii_lowercase().chars().take(16).collect())
                .unwrap_or_default(),
            None => String::new(),
        };
        Ok(PageInfo {
            name: row.name.clone(),
            original_name: row.original_name.clone(),
            uuid: row.uuid.clone(),
            properties,
            aliases,
            tags,
            journal_day: row.journal_day.map(dates::to_iso),
            file: row.file_path.clone(),
            updated_at: row.updated_at,
            block_count: u64::try_from(block_count).unwrap_or(0),
            etag,
            id: row.id,
        })
    }

    fn block_info(
        &self,
        row: &BlockRow,
        names: &mut HashMap<i64, String>,
    ) -> ReaderResult<BlockInfo> {
        let page = match names.get(&row.page_id) {
            Some(n) => n.clone(),
            None => {
                let n = self
                    .api
                    .page_by_id(row.page_id)
                    .map_err(ie)?
                    .map(|p| p.original_name)
                    .unwrap_or_default();
                names.insert(row.page_id, n.clone());
                n
            }
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(row.uuid.as_bytes());
        hasher.update(row.content.as_bytes());
        let version = hasher.finalize().to_hex()[..16].to_owned();
        Ok(BlockInfo {
            uuid: row.uuid.clone(),
            content: row.content.clone(),
            page,
            properties: row.properties.iter().cloned().collect(),
            marker: row.marker.clone(),
            priority: row.priority.clone(),
            scheduled: row.scheduled.map(dates::to_iso),
            deadline: row.deadline.map(dates::to_iso),
            collapsed: row.collapsed,
            depth: u32::try_from(row.depth).unwrap_or(1),
            version,
            children: Vec::new(),
            is_pre_block: row.is_pre_block,
        })
    }

    fn blocks(&self, rows: &[BlockRow]) -> ReaderResult<Vec<BlockInfo>> {
        let mut names = HashMap::new();
        rows.iter()
            .map(|r| self.block_info(r, &mut names))
            .collect()
    }

    fn groups(&self, groups: Vec<RefGroup>) -> ReaderResult<Vec<RefGroupInfo>> {
        let mut names = HashMap::new();
        let mut out = Vec::with_capacity(groups.len());
        for g in groups {
            let mut blocks = Vec::with_capacity(g.blocks.len());
            for hit in &g.blocks {
                blocks.push(RefItem {
                    block: self.block_info(&hit.block, &mut names)?,
                    breadcrumb: hit.breadcrumb.iter().map(|c| c.title.clone()).collect(),
                });
            }
            out.push(RefGroupInfo {
                page: g.page.original_name,
                blocks,
            });
        }
        Ok(out)
    }

    fn page_row(&self, name: &str) -> ReaderResult<Option<PageRow>> {
        let Some(row) = self.api.page_by_name(name).map_err(ie)? else {
            return Ok(None);
        };
        // An alias page without content redirects to the page that declares it.
        match self.api.alias_redirect(row.id).map_err(ie)? {
            Some(target) => Ok(Some(target)),
            None => Ok(Some(row)),
        }
    }

    fn ref_breadcrumb(&self, uuid: &str) -> Vec<String> {
        self.api
            .ancestors(uuid)
            .map(|rows| rows.into_iter().map(|b| b.title).collect())
            .unwrap_or_default()
    }
}

/// Split a `alias::` / `tags::` property value (`[[a]], b, #c`) into page names.
fn split_page_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| {
            s.trim()
                .trim_start_matches('#')
                .trim_start_matches("[[")
                .trim_end_matches("]]")
                .trim()
                .to_owned()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn read_text(path: &Path) -> ReaderResult<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ie(e)),
    }
}

impl GraphReader for IndexGraphReader {
    fn graph_info(&self) -> ReaderResult<GraphInfo> {
        let conn = self.pool.get().map_err(ie)?;
        let stats = IndexStats::read(&conn).map_err(ie)?;
        Ok(GraphInfo {
            name: self.name.clone(),
            path: self.root.display().to_string(),
            page_count: u64::try_from(stats.pages).ok(),
            block_count: u64::try_from(stats.blocks).ok(),
        })
    }

    fn search(&self, q: &SearchQuery) -> ReaderResult<Vec<SearchItem>> {
        let scope = match &q.page {
            Some(name) => match self.page_row(name)? {
                Some(p) => Scope::Page(p.id),
                None => return Ok(Vec::new()),
            },
            None => Scope::All,
        };
        let opts = SearchOptions {
            limit: q.limit.saturating_mul(2).max(q.limit),
            scope,
            today: Some(dates::today_int()),
            ..SearchOptions::default()
        };
        let hits = {
            let conn = self.pool.get().map_err(ie)?;
            search(&conn, &q.query, &opts).map_err(ie)?
        };
        let mut out = Vec::new();
        for hit in hits {
            if out.len() >= q.limit {
                break;
            }
            match hit {
                SearchHit::Page {
                    title,
                    snippet,
                    score,
                    ..
                } if q.kind != SearchKind::Block => out.push(SearchItem {
                    kind: "page".into(),
                    uuid: None,
                    page: title,
                    snippet: snippet.text,
                    score,
                    breadcrumb: Vec::new(),
                }),
                SearchHit::Block {
                    uuid,
                    page_title,
                    snippet,
                    score,
                    ..
                } if q.kind != SearchKind::Page => {
                    let breadcrumb = self.ref_breadcrumb(&uuid);
                    out.push(SearchItem {
                        kind: "block".into(),
                        uuid: Some(uuid),
                        page: page_title,
                        snippet: snippet.text,
                        score,
                        breadcrumb,
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }

    fn page(&self, name: &str) -> ReaderResult<Option<PageInfo>> {
        match self.page_row(name)? {
            Some(row) => Ok(Some(self.page_info(&row)?)),
            None => Ok(None),
        }
    }

    fn list_pages(&self, q: &ListPagesQuery) -> ReaderResult<Vec<PageInfo>> {
        let filter = PageFilter {
            journals: q.journals,
            placeholders: false,
            builtins: false,
        };
        let mut rows = self.api.all_pages(filter, PageSort::Name).map_err(ie)?;
        if let Some(ns) = &q.namespace {
            let prefix = format!("{}/", ns.trim().trim_end_matches('/').to_lowercase());
            rows.retain(|p| p.name.starts_with(&prefix));
        }
        if let Some(since) = q.modified_since {
            rows.retain(|p| p.updated_at.is_some_and(|u| u >= since));
        }
        if let Some(tag) = &q.tag {
            let tag_id = self.api.page_id(tag).map_err(ie)?;
            let ids: HashSet<i64> = match tag_id {
                Some(id) => {
                    let conn = self.pool.get().map_err(ie)?;
                    let mut st = conn
                        .prepare("SELECT page_id FROM page_tags WHERE tag_page_id = ?1")
                        .map_err(ie)?;
                    st.query_map([id], |r| r.get(0))
                        .map_err(ie)?
                        .collect::<Result<_, _>>()
                        .map_err(ie)?
                }
                None => HashSet::new(),
            };
            rows.retain(|p| ids.contains(&p.id));
        }
        rows.iter()
            .skip(q.offset)
            .take(q.limit)
            .map(|r| self.page_info(r))
            .collect()
    }

    fn recent_pages(&self, offset: usize, limit: usize) -> ReaderResult<Vec<PageInfo>> {
        let filter = PageFilter {
            journals: true,
            placeholders: false,
            builtins: false,
        };
        let rows = self.api.all_pages(filter, PageSort::Updated).map_err(ie)?;
        rows.iter()
            .skip(offset)
            .take(limit)
            .map(|r| self.page_info(r))
            .collect()
    }

    fn page_blocks(
        &self,
        page: &PageInfo,
        offset: usize,
        limit: usize,
        skip_collapsed: bool,
    ) -> ReaderResult<Vec<BlockInfo>> {
        let rows = self
            .api
            .outline(page.id, offset, limit, skip_collapsed)
            .map_err(ie)?;
        self.blocks(&rows)
    }

    fn block(&self, uuid: &str) -> ReaderResult<Option<BlockInfo>> {
        match self.api.block(uuid).map_err(ie)? {
            Some(row) => Ok(self.blocks(&[row])?.pop()),
            None => Ok(None),
        }
    }

    fn subtree(&self, uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        let rows = self.api.subtree(uuid).map_err(ie)?;
        self.blocks(&rows)
    }

    fn ancestors(&self, uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        let rows = self.api.ancestors(uuid).map_err(ie)?;
        self.blocks(&rows)
    }

    fn journals(&self, before_day: Option<i64>, limit: usize) -> ReaderResult<Vec<PageInfo>> {
        let rows = self.api.journals(before_day, limit).map_err(ie)?;
        rows.iter().map(|r| self.page_info(r)).collect()
    }

    fn journal(&self, day: i64) -> ReaderResult<Option<PageInfo>> {
        let rows = self.api.journals(Some(day + 1), 1).map_err(ie)?;
        match rows.first().filter(|r| r.journal_day == Some(day)) {
            Some(row) => Ok(Some(self.page_info(row)?)),
            None => Ok(None),
        }
    }

    fn linked_references(&self, page: &PageInfo) -> ReaderResult<Vec<RefGroupInfo>> {
        let groups = self.api.linked_references(page.id).map_err(ie)?;
        self.groups(groups)
    }

    fn unlinked_references(&self, page: &PageInfo) -> ReaderResult<Vec<RefGroupInfo>> {
        let groups = self.api.unlinked_references(page.id).map_err(ie)?;
        self.groups(groups)
    }

    fn block_referrers(&self, uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        let rows = self.api.block_referrers(uuid).map_err(ie)?;
        self.blocks(&rows)
    }

    fn tasks(&self, q: &TaskQuery) -> ReaderResult<Vec<BlockInfo>> {
        let page_id = match &q.page {
            Some(name) => match self.page_row(name)? {
                Some(p) => Some(p.id),
                None => return Ok(Vec::new()),
            },
            None => None,
        };
        let filter = TaskFilter {
            markers: q.markers.clone(),
            priority: q.priority.clone(),
            page_id,
        };
        let items = self.api.tasks(&filter).map_err(ie)?;
        let mut names = HashMap::new();
        items
            .iter()
            .map(|t| self.block_info(&t.block, &mut names))
            .collect()
    }

    fn page_file_text(&self, page: &PageInfo) -> ReaderResult<Option<String>> {
        let Some(rel) = &page.file else {
            return Ok(None);
        };
        let rel = Path::new(rel);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(ReaderError::invalid("page file path escapes the graph"));
        }
        read_text(&self.root.join(rel))
    }

    fn config_text(&self) -> ReaderResult<Option<String>> {
        read_text(&self.root.join("logseq").join("config.edn"))
    }

    fn read_asset(&self, rel: &str, max_bytes: u64) -> ReaderResult<Option<Vec<u8>>> {
        let rel = Path::new(rel);
        if rel.as_os_str().is_empty()
            || rel.is_absolute()
            || rel.components().any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(ReaderError::invalid(
                "asset path must be relative to assets/",
            ));
        }
        let assets = match self.root.join("assets").canonicalize() {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ie(e)),
        };
        let file = match assets.join(rel).canonicalize() {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ie(e)),
        };
        // Symlinks pointing outside assets/ are refused.
        if !file.starts_with(&assets) || !file.is_file() {
            return Err(ReaderError::invalid("asset path escapes assets/"));
        }
        let len = std::fs::metadata(&file).map_err(ie)?.len();
        if len > max_bytes {
            return Err(ReaderError::invalid(format!(
                "asset is {len} bytes, above the {max_bytes} byte limit"
            )));
        }
        std::fs::read(&file).map(Some).map_err(ie)
    }

    fn changes(&self) -> Option<broadcast::Receiver<ChangeEvent>> {
        Some(self.changes.subscribe())
    }
}
