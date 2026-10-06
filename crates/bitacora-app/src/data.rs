//! Read-side data access for the views: turns index rows into render models.
//!
//! Everything here is blocking and GPUI-free; views call it from background tasks. The index is
//! only read (SQLite is a cache, ADR-005): pages, outlines, breadcrumbs, references and
//! journals all come from [`IndexReader`], and `((block refs))` resolve through it too.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use bitacora_config::{Edn, EffectiveConfig};
use bitacora_core::date::Date;
use bitacora_core::journal::journal_page;
use bitacora_index::{BlockRow, IndexEvent, IndexReader, PageRow, RefFilters, RefGroup};
use bitacora_markdown::properties::{PropertyConfig, normalize_key};

use crate::render::inline::BlockResolver;
use crate::render::model::{BlockModel, PropertyRow, Referrer, Row};

/// Default of `:ref/default-open-blocks-level`.
const DEFAULT_REF_OPEN_LEVEL: usize = 2;

/// Settings the views derive from `config.edn`.
#[derive(Debug, Clone)]
pub struct ViewSettings {
    /// Property parsing options.
    pub props: PropertyConfig,
    /// The effective configuration (journal titles, ...).
    pub config: EffectiveConfig,
    /// `:ref/default-open-blocks-level`: how deep reference trees start open.
    pub ref_open_level: usize,
}

impl ViewSettings {
    /// Reads the settings out of a configuration.
    pub fn from_config(cfg: &EffectiveConfig) -> Self {
        let norm = |v: Vec<String>| v.iter().map(|k| normalize_key(k)).collect::<BTreeSet<_>>();
        let level = cfg
            .get("ref/default-open-blocks-level")
            .and_then(Edn::as_int)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_REF_OPEN_LEVEL);
        Self {
            props: PropertyConfig {
                ignored_page_references_keywords: norm(cfg.ignored_page_references_keywords()),
                separated_by_commas: norm(cfg.property_separated_by_commas()),
            },
            config: cfg.clone(),
            ref_open_level: level,
        }
    }
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self::from_config(&EffectiveConfig::default())
    }
}

/// Everything a view needs to read an open graph.
#[derive(Debug, Clone)]
pub struct GraphHandle {
    /// Typed read API over the index.
    pub reader: IndexReader,
    /// The graph folder.
    pub root: PathBuf,
    /// View settings.
    pub settings: Arc<ViewSettings>,
}

impl PartialEq for GraphHandle {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && Arc::ptr_eq(&self.settings, &other.settings)
    }
}

impl Eq for GraphHandle {}

/// Resolves `((uuid))` through the index.
#[derive(Debug)]
pub struct IndexResolver<'a>(pub &'a IndexReader);

impl BlockResolver for IndexResolver<'_> {
    fn resolve(&self, uuid: &str) -> Option<String> {
        let block = self.0.block(uuid).ok().flatten()?;
        Some(block.title)
    }
}

/// A clickable header link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Text shown.
    pub label: String,
    /// Where it goes.
    pub target: crate::nav::Route,
}

/// What the page header shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageHeader {
    /// Display title.
    pub title: String,
    /// Index id of the page (`None` for a name the index does not know).
    pub page_id: Option<i64>,
    /// A journal page.
    pub is_journal: bool,
    /// The page has no file yet.
    pub placeholder: bool,
    /// Page properties (the pre-block), hidden built-ins removed.
    pub properties: Vec<PropertyRow>,
    /// Namespace ancestors, outermost first.
    pub namespace: Vec<Link>,
    /// Direct namespace children.
    pub children: Vec<Link>,
    /// The alias the user asked for when the page shown is its target.
    pub redirected_from: Option<String>,
    /// Breadcrumb of a zoomed block: page, ancestors.
    pub zoom: Vec<Link>,
}

/// A loaded page (first chunk).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageLoad {
    /// Header data.
    pub header: PageHeader,
    /// Rows of the first chunk, without the page-properties pre-block.
    pub rows: Vec<Row>,
    /// Blocks fetched so far (the offset of the next chunk).
    pub fetched: usize,
    /// More blocks exist after `fetched`.
    pub has_more: bool,
}

/// Blocks per chunk after the first one.
pub const CHUNK: usize = 25;
/// Blocks in the first chunk.
pub const FIRST_CHUNK: usize = 50;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Builds rows from index blocks. `base_depth` is the absolute depth (1 = top level) rendered
/// at depth 0.
pub fn rows_from_blocks(h: &GraphHandle, blocks: &[BlockRow], base_depth: i64) -> Vec<Row> {
    let resolver = IndexResolver(&h.reader);
    blocks
        .iter()
        .map(|b| {
            let block = BlockModel::from_content(&b.content, &h.settings.props, &resolver);
            // Only blocks with an `id::` property can be referenced from files.
            let ref_count = if block.id.is_some() {
                h.reader.block_ref_count(&b.uuid).unwrap_or(0)
            } else {
                0
            };
            Row {
                block_index: (!b.is_pre_block).then(|| usize::try_from(b.ord).unwrap_or(0)),
                depth: usize::try_from((b.depth - 1 - base_depth).max(0)).unwrap_or(0),
                has_children: b.subtree_end > b.ord,
                block,
                uuid: Some(b.uuid.clone()),
                view_collapsed: None,
                ref_count,
                referrers: None,
            }
        })
        .collect()
}

fn link_to_page(p: &PageRow) -> Link {
    Link {
        label: p
            .original_name
            .rsplit('/')
            .next()
            .unwrap_or(&p.original_name)
            .to_owned(),
        target: crate::nav::Route::Page(p.original_name.clone()),
    }
}

fn header_of(h: &GraphHandle, page: &PageRow) -> Result<PageHeader, String> {
    let mut namespace = Vec::new();
    let mut parent = page.namespace_parent_id;
    // Namespaces are shallow; the bound only guards against a cycle in a corrupt index.
    for _ in 0..16 {
        let Some(id) = parent else { break };
        let Some(p) = h.reader.page_by_id(id).map_err(err)? else {
            break;
        };
        parent = p.namespace_parent_id;
        namespace.push(link_to_page(&p));
    }
    namespace.reverse();
    let children = h
        .reader
        .namespace_children(page.id)
        .map_err(err)?
        .iter()
        .map(|p| Link {
            label: p.original_name.clone(),
            target: crate::nav::Route::Page(p.original_name.clone()),
        })
        .collect();
    Ok(PageHeader {
        title: page.original_name.clone(),
        page_id: Some(page.id),
        is_journal: page.is_journal,
        placeholder: page.is_placeholder(),
        namespace,
        children,
        ..PageHeader::default()
    })
}

/// Splits the page-properties pre-block off the first rows into header properties.
fn take_properties(rows: &mut Vec<Row>, header: &mut PageHeader) {
    if rows.first().is_some_and(|r| r.block_index.is_none()) {
        let pre = rows.remove(0);
        header.properties = pre
            .block
            .properties
            .into_iter()
            .filter(|p| {
                let key = normalize_key(&p.key);
                key != "title" && key != "filters"
            })
            .collect();
    }
}

/// Opens a page by title: header (alias redirect, namespaces, properties) and the first chunk.
/// A title the index does not know shows as an empty placeholder.
pub fn open_page(h: &GraphHandle, name: &str, limit: usize) -> Result<PageLoad, String> {
    let Some(mut page) = h.reader.page_by_name(name).map_err(err)? else {
        return Ok(PageLoad {
            header: PageHeader {
                title: name.to_owned(),
                placeholder: true,
                ..PageHeader::default()
            },
            ..PageLoad::default()
        });
    };
    let mut redirected_from = None;
    if let Some(target) = h.reader.alias_redirect(page.id).map_err(err)? {
        redirected_from = Some(page.original_name.clone());
        page = target;
    }
    let mut header = header_of(h, &page)?;
    header.redirected_from = redirected_from;
    let blocks = h.reader.outline(page.id, 0, limit, false).map_err(err)?;
    let fetched = blocks.len();
    let mut rows = rows_from_blocks(h, &blocks, 0);
    take_properties(&mut rows, &mut header);
    Ok(PageLoad {
        header,
        rows,
        fetched,
        has_more: fetched >= limit,
    })
}

/// The next chunk of a page: rows, new fetched count and whether more remain.
pub fn page_chunk(
    h: &GraphHandle,
    page_id: i64,
    offset: usize,
    limit: usize,
) -> Result<(Vec<Row>, usize, bool), String> {
    let blocks = h
        .reader
        .outline(page_id, offset, limit, false)
        .map_err(err)?;
    let fetched = offset + blocks.len();
    let rows = rows_from_blocks(h, &blocks, 0);
    Ok((rows, fetched, blocks.len() >= limit))
}

/// Rows kept for a zoomed block (a hard cap against pathological subtrees).
const ZOOM_CAP: usize = 5000;

/// A block zoomed in: breadcrumb plus the block and its descendants.
pub fn zoom_block(h: &GraphHandle, uuid: &str) -> Result<PageLoad, String> {
    let block = h
        .reader
        .block(uuid)
        .map_err(err)?
        .ok_or_else(|| format!("block {uuid} not found"))?;
    let page = h
        .reader
        .page_by_id(block.page_id)
        .map_err(err)?
        .ok_or_else(|| "the block's page is gone".to_owned())?;
    let mut header = header_of(h, &page)?;
    header.zoom.push(Link {
        label: page.original_name.clone(),
        target: crate::nav::Route::Page(page.original_name.clone()),
    });
    for a in h.reader.ancestors(uuid).map_err(err)? {
        header.zoom.push(Link {
            label: a.title.clone(),
            target: crate::nav::Route::Block(a.uuid.clone()),
        });
    }
    header.properties.clear();
    let mut blocks = h.reader.subtree(uuid).map_err(err)?;
    blocks.truncate(ZOOM_CAP);
    let rows = rows_from_blocks(h, &blocks, block.depth - 1);
    let fetched = rows.len();
    Ok(PageLoad {
        header,
        rows,
        fetched,
        has_more: false,
    })
}

/// The blocks that reference `uuid`, with their page titles.
pub fn load_referrers(h: &GraphHandle, uuid: &str) -> Result<Vec<Referrer>, String> {
    let blocks = h.reader.block_referrers(uuid).map_err(err)?;
    let mut pages: HashMap<i64, String> = HashMap::new();
    let mut out = Vec::with_capacity(blocks.len());
    for b in blocks {
        let page = match pages.get(&b.page_id) {
            Some(p) => p.clone(),
            None => {
                let name = h
                    .reader
                    .page_by_id(b.page_id)
                    .map_err(err)?
                    .map(|p| p.original_name)
                    .unwrap_or_default();
                pages.insert(b.page_id, name.clone());
                name
            }
        };
        out.push(Referrer {
            uuid: b.uuid,
            page,
            title: b.title,
        });
    }
    Ok(out)
}

/// One matching block of a reference group, with its context.
#[derive(Debug, Clone, PartialEq)]
pub struct RefHitModel {
    /// Ancestors as `(uuid, title)`, outermost first.
    pub crumbs: Vec<(String, String)>,
    /// The block and its descendants, collapsed beyond the default open level.
    pub rows: Vec<Row>,
}

/// References from one page.
#[derive(Debug, Clone, PartialEq)]
pub struct RefGroupModel {
    /// Title of the referencing page.
    pub page: String,
    /// A journal page.
    pub is_journal: bool,
    /// Matching blocks.
    pub hits: Vec<RefHitModel>,
}

/// Linked or unlinked references of a page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RefsLoad {
    /// Groups by page.
    pub groups: Vec<RefGroupModel>,
    /// Matching blocks in all groups.
    pub total: usize,
}

fn refs_model(h: &GraphHandle, groups: Vec<RefGroup>) -> Result<RefsLoad, String> {
    let level = h.settings.ref_open_level;
    let mut total = 0;
    let mut out = Vec::with_capacity(groups.len());
    for g in groups {
        let mut hits = Vec::with_capacity(g.blocks.len());
        for hit in g.blocks {
            let blocks = h.reader.subtree(&hit.block.uuid).map_err(err)?;
            let mut rows = rows_from_blocks(h, &blocks, hit.block.depth - 1);
            for row in &mut rows {
                if row.depth + 1 >= level && row.has_children && row.view_collapsed.is_none() {
                    row.view_collapsed = Some(true);
                }
            }
            hits.push(RefHitModel {
                crumbs: hit
                    .breadcrumb
                    .into_iter()
                    .map(|c| (c.uuid, c.title))
                    .collect(),
                rows,
            });
            total += 1;
        }
        out.push(RefGroupModel {
            page: g.page.original_name,
            is_journal: g.page.is_journal,
            hits,
        });
    }
    Ok(RefsLoad { groups: out, total })
}

/// The `filters::` property of a page.
pub fn page_filters(h: &GraphHandle, page_id: i64) -> Result<RefFilters, String> {
    h.reader.page_ref_filters(page_id).map_err(err)
}

/// Linked references under `filters`.
pub fn load_linked(
    h: &GraphHandle,
    page_id: i64,
    filters: &RefFilters,
) -> Result<RefsLoad, String> {
    let groups = h
        .reader
        .linked_references_with(page_id, filters)
        .map_err(err)?;
    refs_model(h, groups)
}

/// Unlinked references (plain-text mentions).
pub fn load_unlinked(h: &GraphHandle, page_id: i64) -> Result<RefsLoad, String> {
    let groups = h.reader.unlinked_references(page_id).map_err(err)?;
    refs_model(h, groups)
}

/// Whether an index event may have changed what a page view shows.
pub fn event_touches(event: &IndexEvent, page_id: Option<i64>, file: Option<&str>) -> bool {
    match event {
        IndexEvent::FileReplaced {
            page_ids_touched,
            path,
            ..
        } => {
            page_id.is_some_and(|id| page_ids_touched.contains(&id)) || file == Some(path.as_str())
        }
        IndexEvent::FileDeleted {
            page_ids_touched,
            path,
            ..
        } => {
            page_id.is_some_and(|id| page_ids_touched.contains(&id)) || file == Some(path.as_str())
        }
        IndexEvent::FileRenamed { from, to } => {
            file == Some(from.as_str()) || file == Some(to.as_str())
        }
        IndexEvent::BulkFinished => true,
    }
}

/// Today's date in the local time zone.
pub fn today_local() -> Option<Date> {
    let date = jiff::Zoned::now().date();
    Date::new(
        i32::from(date.year()),
        u8::try_from(date.month()).ok()?,
        u8::try_from(date.day()).ok()?,
    )
}

/// Title of the journal for `date` according to `:journal/page-title-format`.
pub fn journal_title(h: &GraphHandle, date: Date) -> String {
    journal_page(date, &h.settings.config).title
}

/// A day in the journals feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalDay {
    /// `yyyyMMdd`.
    pub day: u32,
    /// Display title.
    pub title: String,
    /// Index id; `None` for the virtual page of a day without a file.
    pub page_id: Option<i64>,
}

/// Today's entry: the real page when its file exists, else a virtual one (nothing is written).
pub fn today_entry(h: &GraphHandle, today: Date) -> Result<JournalDay, String> {
    let title = journal_title(h, today);
    let page = h
        .reader
        .page_by_name(&title)
        .map_err(err)?
        .filter(|p| !p.is_placeholder());
    Ok(JournalDay {
        day: today.journal_day(),
        title,
        page_id: page.map(|p| p.id),
    })
}

/// Past journals (newest first) strictly before `before` (`yyyyMMdd`), never later than
/// `today`. The bool tells whether more may follow.
pub fn journal_days(
    h: &GraphHandle,
    today: Date,
    before: Option<u32>,
    limit: usize,
) -> Result<(Vec<JournalDay>, bool), String> {
    let today_day = today.journal_day();
    // `before` is exclusive; one past today admits today and everything older.
    let bound = before.unwrap_or(today_day + 1).min(today_day + 1);
    let rows = h
        .reader
        .journals(Some(i64::from(bound)), limit)
        .map_err(err)?;
    let more = rows.len() >= limit;
    let days = rows
        .into_iter()
        .filter_map(|p| {
            Some(JournalDay {
                day: u32::try_from(p.journal_day?).ok()?,
                title: p.original_name,
                page_id: Some(p.id),
            })
        })
        .collect();
    Ok((days, more))
}

/// First rows of a journal (properties pre-block kept as a row) for the feed.
pub fn journal_rows(
    h: &GraphHandle,
    page_id: i64,
    limit: usize,
) -> Result<(Vec<Row>, bool), String> {
    let blocks = h.reader.outline(page_id, 0, limit, false).map_err(err)?;
    let more = blocks.len() >= limit;
    Ok((rows_from_blocks(h, &blocks, 0), more))
}
