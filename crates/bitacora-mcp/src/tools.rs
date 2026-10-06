//! Read tool implementations: argument/result types and pure functions over a [`GraphReader`].
//! The rmcp glue (scope checks, `spawn_blocking`, result encoding) lives in `handler`.

use std::fmt::Write as _;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dates;
use crate::query;
use crate::reader::{
    BlockInfo, GraphInfo, GraphReader, ListPagesQuery, PageInfo, RefGroupInfo, SearchItem,
    SearchKind, SearchQuery, TaskQuery,
};
use crate::render::{
    Code, ToolError, ToolResult, decode_cursor, drop_collapsed, encode_cursor, limit, nest, output,
    render_block_line, render_tree, strip_properties,
};
use crate::status::SyncStatus;

/// Ensure the optional `graph` argument names the served graph.
pub(crate) fn check_graph(r: &dyn GraphReader, graph: Option<&str>) -> Result<(), ToolError> {
    let Some(g) = graph.filter(|g| !g.is_empty()) else {
        return Ok(());
    };
    let info = r.graph_info()?;
    if g.eq_ignore_ascii_case(&info.name) || g == info.path {
        Ok(())
    } else {
        Err(ToolError::not_found(format!("graph `{g}`")))
    }
}

fn find_page(r: &dyn GraphReader, name: &str) -> Result<PageInfo, ToolError> {
    if name.trim().is_empty() {
        return Err(ToolError::invalid("`name` must not be empty"));
    }
    r.page(name.trim())?
        .ok_or_else(|| ToolError::not_found(format!("page `{name}`")))
}

fn page_summary(p: &PageInfo) -> String {
    let mut s = format!("# {}\n", p.original_name);
    if let Some(day) = &p.journal_day {
        let _ = writeln!(s, "- journal: {day}");
    }
    if let Some(f) = &p.file {
        let _ = writeln!(s, "- file: {f}");
    }
    let _ = writeln!(s, "- blocks: {}", p.block_count);
    if !p.aliases.is_empty() {
        let _ = writeln!(s, "- aliases: {}", p.aliases.join(", "));
    }
    if !p.tags.is_empty() {
        let _ = writeln!(s, "- tags: {}", p.tags.join(", "));
    }
    for (k, v) in &p.properties {
        let _ = writeln!(s, "- {k}:: {v}");
    }
    s
}

// ---------------------------------------------------------------- search

/// Arguments of `search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct SearchArgs {
    /// Search text (Logseq `search`).
    pub query: String,
    /// Maximum hits (default 20, max 100).
    pub limit: Option<u32>,
    /// `page`, `block` or `all` (default).
    pub kind: Option<SearchKind>,
    /// Restrict to the blocks of this page.
    pub page: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `search`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct SearchOut {
    pub query: String,
    pub hits: Vec<SearchItem>,
    pub truncated: bool,
}

pub(crate) fn search(r: &dyn GraphReader, a: SearchArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    if a.query.trim().is_empty() {
        return Err(ToolError::invalid("`query` must not be empty"));
    }
    let n = limit(a.limit, 20, 100);
    let hits = r.search(&SearchQuery {
        query: a.query.clone(),
        limit: n,
        kind: a.kind.unwrap_or_default(),
        page: a.page.clone(),
    })?;
    let mut md = format!("Search results for `{}`:\n\n", a.query);
    for h in &hits {
        let snippet = h.snippet.replace('\n', " ");
        if h.kind == "page" {
            let _ = writeln!(md, "- [[{}]] (page): {snippet}", h.page);
        } else {
            let crumbs = if h.breadcrumb.is_empty() {
                String::new()
            } else {
                format!(" > {}", h.breadcrumb.join(" > "))
            };
            let _ = writeln!(
                md,
                "- {snippet}  _(page: [[{}]]{crumbs}, id: {})_",
                h.page,
                h.uuid.as_deref().unwrap_or("")
            );
        }
    }
    if hits.is_empty() {
        md.push_str("No results.\n");
    }
    let truncated = hits.len() >= n;
    output(
        &SearchOut {
            query: a.query,
            hits,
            truncated,
        },
        md,
    )
}

// ---------------------------------------------------------------- pages

/// Arguments of `get_page`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct GetPageArgs {
    /// Page name or alias, case-insensitive (Logseq `Editor.getPage`).
    pub name: String,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

pub(crate) fn get_page(r: &dyn GraphReader, a: GetPageArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let page = find_page(r, &a.name)?;
    let md = page_summary(&page);
    output(&page, md)
}

/// Arguments of `list_pages`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ListPagesArgs {
    /// Only pages under this namespace (`Logseq Editor.getPagesFromNamespace`).
    pub namespace: Option<String>,
    /// Only pages tagged with this page.
    pub tag: Option<String>,
    /// Only pages modified on or after this date (`yyyy-mm-dd`).
    pub modified_since: Option<String>,
    /// Include journal pages (default false).
    pub include_journals: Option<bool>,
    /// Page size (default 50, max 200).
    pub limit: Option<u32>,
    /// `next_cursor` of the previous result.
    pub cursor: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `list_pages`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ListPagesOut {
    pub pages: Vec<PageInfo>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

pub(crate) fn list_pages(r: &dyn GraphReader, a: ListPagesArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let n = limit(a.limit, 50, 200);
    let offset = decode_cursor(a.cursor.as_deref())?;
    let modified_since = match &a.modified_since {
        Some(d) => {
            let day = dates::parse(d)
                .ok_or_else(|| ToolError::invalid("`modified_since` must be yyyy-mm-dd"))?;
            Some(
                dates::day_start_ms(day)
                    .ok_or_else(|| ToolError::invalid("`modified_since` is out of range"))?,
            )
        }
        None => None,
    };
    let mut pages = r.list_pages(&ListPagesQuery {
        namespace: a.namespace,
        tag: a.tag,
        modified_since,
        journals: a.include_journals.unwrap_or(false),
        offset,
        limit: n + 1,
    })?;
    let more = pages.len() > n;
    pages.truncate(n);
    let next_cursor = more.then(|| encode_cursor(offset + n));
    let mut md = String::from("Pages:\n\n");
    for p in &pages {
        let _ = writeln!(md, "- [[{}]] ({} blocks)", p.original_name, p.block_count);
    }
    if let Some(c) = &next_cursor {
        let _ = writeln!(md, "\nMore results: cursor `{c}`");
    }
    output(
        &ListPagesOut {
            truncated: more,
            pages,
            next_cursor,
        },
        md,
    )
}

// ---------------------------------------------------------------- trees

/// Arguments of `get_page_blocks_tree` (`name` or `uuid`).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TreeArgs {
    /// Page name or alias (Logseq `Editor.getPageBlocksTree`).
    pub name: Option<String>,
    /// Block UUID to return a subtree of (Logseq `Editor.getBlock` with children).
    pub uuid: Option<String>,
    /// Maximum depth to return (1 = only top-level blocks).
    pub max_depth: Option<u32>,
    /// Include property data and `key:: value` lines (default true).
    pub include_properties: Option<bool>,
    /// Include children of collapsed blocks (default true).
    pub collapsed_children: Option<bool>,
    /// Maximum blocks per call (default 500, max 2000).
    pub limit: Option<u32>,
    /// `next_cursor` of the previous result (page mode only).
    pub cursor: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `get_block_tree`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct BlockTreeArgs {
    /// Block UUID.
    pub uuid: String,
    /// Maximum depth to return, relative to the block (1 = the block itself).
    pub max_depth: Option<u32>,
    /// Include property data and `key:: value` lines (default true).
    pub include_properties: Option<bool>,
    /// Include children of collapsed blocks (default true).
    pub collapsed_children: Option<bool>,
    /// Maximum blocks (default 500, max 2000).
    pub limit: Option<u32>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of the tree tools.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct TreeOut {
    /// Display name of the page.
    pub page: Option<String>,
    /// Root block UUID (block mode).
    pub root: Option<String>,
    /// Page etag (page mode).
    pub etag: Option<String>,
    /// Nested blocks.
    pub blocks: Vec<BlockInfo>,
    /// Logseq Markdown rendering of `blocks`.
    pub markdown: String,
    /// More blocks exist than were returned.
    pub truncated: bool,
    /// Pass as `cursor` to continue (page mode).
    pub next_cursor: Option<String>,
}

pub(crate) fn get_page_blocks_tree(r: &dyn GraphReader, a: TreeArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    match (&a.name, &a.uuid) {
        (Some(_), Some(_)) | (None, None) => {
            Err(ToolError::invalid("pass exactly one of `name` or `uuid`"))
        }
        (None, Some(uuid)) => block_tree(
            r,
            BlockTreeArgs {
                uuid: uuid.clone(),
                max_depth: a.max_depth,
                include_properties: a.include_properties,
                collapsed_children: a.collapsed_children,
                limit: a.limit,
                graph: None,
            },
        ),
        (Some(name), None) => {
            let page = find_page(r, name)?;
            let n = limit(a.limit, 500, 2000);
            let offset = decode_cursor(a.cursor.as_deref())?;
            let collapsed = a.collapsed_children.unwrap_or(true);
            // The reader counts the pre-block in `offset`; fetch one extra to detect more.
            let mut flat = r.page_blocks(&page, offset, n + 1, !collapsed)?;
            let more = flat.len() > n;
            flat.truncate(n);
            let consumed = flat.len();
            flat.retain(|b| !b.is_pre_block);
            if let Some(max) = a.max_depth {
                flat.retain(|b| b.depth <= max.max(1));
            }
            finish_tree(
                flat,
                Some(page.original_name.clone()),
                None,
                Some(page.etag.clone()),
                a.include_properties.unwrap_or(true),
                more,
                more.then(|| encode_cursor(offset + consumed)),
            )
        }
    }
}

pub(crate) fn block_tree(r: &dyn GraphReader, a: BlockTreeArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let mut flat = r.subtree(&a.uuid)?;
    if flat.is_empty() {
        return Err(ToolError::not_found(format!("block `{}`", a.uuid)));
    }
    if !a.collapsed_children.unwrap_or(true) {
        flat = drop_collapsed(flat);
    }
    let base = flat[0].depth;
    if let Some(max) = a.max_depth {
        flat.retain(|b| b.depth - base < max.max(1));
    }
    let n = limit(a.limit, 500, 2000);
    let more = flat.len() > n;
    flat.truncate(n);
    let page = flat.first().map(|b| b.page.clone());
    let root = flat.first().map(|b| b.uuid.clone());
    finish_tree(
        flat,
        page,
        root,
        None,
        a.include_properties.unwrap_or(true),
        more,
        None,
    )
}

fn finish_tree(
    mut flat: Vec<BlockInfo>,
    page: Option<String>,
    root: Option<String>,
    etag: Option<String>,
    include_properties: bool,
    truncated: bool,
    next_cursor: Option<String>,
) -> ToolResult {
    if !include_properties {
        flat.iter_mut().for_each(strip_properties);
    }
    let blocks = nest(flat);
    let markdown = render_tree(&blocks);
    let mut text = markdown.clone();
    if let Some(c) = &next_cursor {
        let _ = write!(text, "\n[more blocks: cursor `{c}`]\n");
    }
    output(
        &TreeOut {
            page,
            root,
            etag,
            blocks,
            markdown,
            truncated,
            next_cursor,
        },
        text,
    )
}

/// Arguments of `get_block`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct GetBlockArgs {
    /// Block UUID (Logseq `Editor.getBlock`).
    pub uuid: String,
    /// Include nested children (default false).
    pub include_children: Option<bool>,
    /// Include the ancestor chain (default false).
    pub include_parents: Option<bool>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `get_block`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct GetBlockOut {
    pub block: BlockInfo,
    /// Ancestors, outermost first.
    pub parents: Vec<BlockInfo>,
}

pub(crate) fn get_block(r: &dyn GraphReader, a: GetBlockArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let block = if a.include_children.unwrap_or(false) {
        let mut flat = r.subtree(&a.uuid)?;
        flat.truncate(2000);
        nest(flat).into_iter().next()
    } else {
        r.block(&a.uuid)?
    }
    .ok_or_else(|| ToolError::not_found(format!("block `{}`", a.uuid)))?;
    let parents = if a.include_parents.unwrap_or(false) {
        r.ancestors(&a.uuid)?
    } else {
        Vec::new()
    };
    let mut md = String::new();
    if !parents.is_empty() {
        let crumbs: Vec<&str> = parents
            .iter()
            .map(|p| p.content.lines().next().unwrap_or(""))
            .collect();
        let _ = writeln!(md, "Path: {}\n", crumbs.join(" > "));
    }
    md.push_str(&render_tree(std::slice::from_ref(&block)));
    let _ = write!(
        md,
        "\n_(page: [[{}]], id: {}, version: {})_\n",
        block.page, block.uuid, block.version
    );
    output(&GetBlockOut { block, parents }, md)
}

// ---------------------------------------------------------------- journals

/// Arguments of `list_journals`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ListJournalsArgs {
    /// Oldest day to include (`yyyy-mm-dd`).
    pub from: Option<String>,
    /// Newest day to include (`yyyy-mm-dd`); default newest.
    pub to: Option<String>,
    /// Page size (default 7, max 100).
    pub limit: Option<u32>,
    /// `next_cursor` of the previous result.
    pub cursor: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `list_journals`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ListJournalsOut {
    pub journals: Vec<PageInfo>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

fn parse_day(label: &str, v: &str) -> Result<i64, ToolError> {
    dates::parse(v).ok_or_else(|| ToolError::invalid(format!("`{label}` must be yyyy-mm-dd")))
}

pub(crate) fn list_journals(r: &dyn GraphReader, a: ListJournalsArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let n = limit(a.limit, 7, 100);
    let from = a
        .from
        .as_deref()
        .map(|v| parse_day("from", v))
        .transpose()?;
    let mut before = match &a.to {
        Some(v) => Some(parse_day("to", v)? + 1),
        None => None,
    };
    if let Some(c) = a.cursor.as_deref().filter(|c| !c.is_empty()) {
        before = Some(
            c.strip_prefix("d:")
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| ToolError::invalid("invalid cursor"))?,
        );
    }
    let mut journals = r.journals(before, n + 1)?;
    if let Some(from) = from {
        journals.retain(|p| {
            p.journal_day
                .as_deref()
                .and_then(dates::parse)
                .is_some_and(|d| d >= from)
        });
    }
    let more = journals.len() > n;
    journals.truncate(n);
    let next_cursor = if more {
        journals
            .last()
            .and_then(|p| p.journal_day.as_deref())
            .and_then(dates::parse)
            .map(|d| format!("d:{d}"))
    } else {
        None
    };
    let mut md = String::from("Journals (newest first):\n\n");
    for p in &journals {
        let _ = writeln!(
            md,
            "- {} [[{}]] ({} blocks)",
            p.journal_day.as_deref().unwrap_or(""),
            p.original_name,
            p.block_count
        );
    }
    output(
        &ListJournalsOut {
            truncated: more,
            journals,
            next_cursor,
        },
        md,
    )
}

/// Arguments of `get_today_journal`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TodayArgs {
    /// Create the page when missing; refused until writes are enabled.
    pub create_if_missing: Option<bool>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `get_today_journal`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct TodayOut {
    /// `yyyy-mm-dd`.
    pub date: String,
    pub exists: bool,
    pub page: Option<PageInfo>,
    pub blocks: Vec<BlockInfo>,
    pub markdown: String,
    pub truncated: bool,
}

pub(crate) fn get_today_journal(r: &dyn GraphReader, a: TodayArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    if a.create_if_missing.unwrap_or(false) {
        return Err(ToolError::new(
            Code::ReadOnly,
            "creating the journal page needs write access, which is not enabled",
        ));
    }
    let day = r.today();
    let date = dates::to_iso(day);
    let Some(page) = r.journal(day)? else {
        let md = format!("No journal page for {date} yet.\n");
        return output(
            &TodayOut {
                date,
                exists: false,
                page: None,
                blocks: Vec::new(),
                markdown: String::new(),
                truncated: false,
            },
            md,
        );
    };
    let mut flat = r.page_blocks(&page, 0, 501, false)?;
    let truncated = flat.len() > 500;
    flat.truncate(500);
    flat.retain(|b| !b.is_pre_block);
    let blocks = nest(flat);
    let markdown = render_tree(&blocks);
    output(
        &TodayOut {
            date,
            exists: true,
            page: Some(page),
            blocks,
            markdown: markdown.clone(),
            truncated,
        },
        markdown,
    )
}

// ---------------------------------------------------------------- backlinks, tasks

/// Arguments of `backlinks`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct BacklinksArgs {
    /// Page name or alias (Logseq `Editor.getPageLinkedReferences`).
    pub name: Option<String>,
    /// Block UUID: blocks that reference it with `((uuid))`.
    pub uuid: Option<String>,
    /// Also list plain-text mentions of the page (default false; page mode only).
    pub include_unlinked: Option<bool>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `backlinks`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct BacklinksOut {
    pub target: String,
    pub groups: Vec<RefGroupInfo>,
    pub unlinked: Vec<RefGroupInfo>,
    pub total: usize,
    pub truncated: bool,
}

const MAX_REF_BLOCKS: usize = 200;

fn cap_groups(groups: &mut Vec<RefGroupInfo>, budget: &mut usize) -> bool {
    let mut cut = false;
    for g in groups.iter_mut() {
        if g.blocks.len() > *budget {
            g.blocks.truncate(*budget);
            cut = true;
        }
        *budget -= g.blocks.len();
    }
    groups.retain(|g| !g.blocks.is_empty());
    cut
}

fn render_groups(md: &mut String, title: &str, groups: &[RefGroupInfo]) {
    if groups.is_empty() {
        return;
    }
    let _ = writeln!(md, "## {title}\n");
    for g in groups {
        let _ = writeln!(md, "### [[{}]]", g.page);
        for item in &g.blocks {
            let first = item.block.content.lines().next().unwrap_or("");
            if item.breadcrumb.is_empty() {
                let _ = writeln!(md, "- {first}  _(id: {})_", item.block.uuid);
            } else {
                let _ = writeln!(
                    md,
                    "- {first}  _(under: {}; id: {})_",
                    item.breadcrumb.join(" > "),
                    item.block.uuid
                );
            }
        }
        md.push('\n');
    }
}

pub(crate) fn backlinks(r: &dyn GraphReader, a: BacklinksArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let mut budget = MAX_REF_BLOCKS;
    let (target, mut groups, mut unlinked) = match (&a.name, &a.uuid) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(ToolError::invalid("pass exactly one of `name` or `uuid`"));
        }
        (Some(name), None) => {
            let page = find_page(r, name)?;
            let groups = r.linked_references(&page)?;
            let unlinked = if a.include_unlinked.unwrap_or(false) {
                r.unlinked_references(&page)?
            } else {
                Vec::new()
            };
            (page.original_name, groups, unlinked)
        }
        (None, Some(uuid)) => {
            if r.block(uuid)?.is_none() {
                return Err(ToolError::not_found(format!("block `{uuid}`")));
            }
            let mut by_page: Vec<RefGroupInfo> = Vec::new();
            for b in r.block_referrers(uuid)? {
                let item = crate::reader::RefItem {
                    breadcrumb: r
                        .ancestors(&b.uuid)?
                        .into_iter()
                        .map(|p| p.content.lines().next().unwrap_or("").to_owned())
                        .collect(),
                    block: b,
                };
                match by_page.iter_mut().find(|g| g.page == item.block.page) {
                    Some(g) => g.blocks.push(item),
                    None => by_page.push(RefGroupInfo {
                        page: item.block.page.clone(),
                        blocks: vec![item],
                    }),
                }
            }
            (format!("(({uuid}))"), by_page, Vec::new())
        }
    };
    let count = |gs: &[RefGroupInfo]| gs.iter().map(|g| g.blocks.len()).sum::<usize>();
    let total = count(&groups) + count(&unlinked);
    let cut_a = cap_groups(&mut groups, &mut budget);
    let cut_b = cap_groups(&mut unlinked, &mut budget);
    let mut md = format!("Backlinks of {target}: {total} block(s).\n\n");
    render_groups(&mut md, "Linked references", &groups);
    render_groups(&mut md, "Unlinked references", &unlinked);
    output(
        &BacklinksOut {
            target,
            groups,
            unlinked,
            total,
            truncated: cut_a || cut_b,
        },
        md,
    )
}

/// Arguments of `tasks`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TasksArgs {
    /// Markers to keep: TODO, DOING, NOW, LATER, WAITING, DONE, CANCELED (default: all).
    pub status: Option<Vec<String>>,
    /// Only tasks on this page.
    pub page: Option<String>,
    /// Only tasks scheduled on or before this day (`yyyy-mm-dd`).
    pub scheduled_before: Option<String>,
    /// Only tasks due on or before this day (`yyyy-mm-dd`).
    pub deadline_before: Option<String>,
    /// Priority `A`, `B` or `C`.
    pub priority: Option<String>,
    /// Page size (default 50, max 500).
    pub limit: Option<u32>,
    /// `next_cursor` of the previous result.
    pub cursor: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `tasks`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct TasksOut {
    pub tasks: Vec<BlockInfo>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

fn expand_markers(status: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for s in status {
        let up = s.trim().to_ascii_uppercase();
        match up.as_str() {
            "CANCELED" | "CANCELLED" => {
                out.push("CANCELED".to_owned());
                out.push("CANCELLED".to_owned());
            }
            "WAITING" | "WAIT" => {
                out.push("WAITING".to_owned());
                out.push("WAIT".to_owned());
            }
            _ => out.push(up),
        }
    }
    out
}

pub(crate) fn tasks(r: &dyn GraphReader, a: TasksArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let n = limit(a.limit, 50, 500);
    let offset = decode_cursor(a.cursor.as_deref())?;
    let priority = match a.priority.as_deref() {
        None => None,
        Some(p) if matches!(p.to_ascii_uppercase().as_str(), "A" | "B" | "C") => {
            Some(p.to_ascii_uppercase())
        }
        Some(_) => return Err(ToolError::invalid("`priority` must be A, B or C")),
    };
    let iso = |label: &str, v: &Option<String>| -> Result<Option<String>, ToolError> {
        v.as_deref()
            .map(|s| parse_day(label, s).map(dates::to_iso))
            .transpose()
    };
    let sched = iso("scheduled_before", &a.scheduled_before)?;
    let dead = iso("deadline_before", &a.deadline_before)?;
    let mut tasks = r.tasks(&TaskQuery {
        markers: expand_markers(a.status.as_deref().unwrap_or(&[])),
        priority,
        page: a.page,
    })?;
    if let Some(s) = &sched {
        tasks.retain(|t| t.scheduled.as_ref().is_some_and(|d| d <= s));
    }
    if let Some(d) = &dead {
        tasks.retain(|t| t.deadline.as_ref().is_some_and(|x| x <= d));
    }
    let total = tasks.len();
    let page: Vec<BlockInfo> = tasks.into_iter().skip(offset).take(n).collect();
    let more = offset + page.len() < total;
    let next_cursor = more.then(|| encode_cursor(offset + page.len()));
    let mut md = format!("{total} task(s):\n\n");
    for t in &page {
        md.push_str(&render_block_line(t));
    }
    if let Some(c) = &next_cursor {
        let _ = writeln!(md, "\nMore results: cursor `{c}`");
    }
    output(
        &TasksOut {
            tasks: page,
            next_cursor,
            truncated: more,
        },
        md,
    )
}

// ---------------------------------------------------------------- query

/// Arguments of `query`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct QueryArgs {
    /// Logseq simple query, e.g. `(and (task TODO) [[Project X]])`. Supported now: `and`,
    /// `(task ...)`, `(priority ...)`, `[[page]]` and `"text"`; datalog is rejected.
    pub dsl: String,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `query`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct QueryOut {
    pub blocks: Vec<BlockInfo>,
    pub truncated: bool,
}

pub(crate) fn query_tool(r: &dyn GraphReader, a: QueryArgs) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let expr = query::parse(&a.dsl)?;
    let mut blocks = query::run(r, &expr)?;
    let truncated = blocks.len() > query::MAX_RESULTS;
    blocks.truncate(query::MAX_RESULTS);
    let mut md = format!("{} block(s) matched.\n\n", blocks.len());
    for b in &blocks {
        md.push_str(&render_block_line(b));
    }
    output(&QueryOut { blocks, truncated }, md)
}

// ---------------------------------------------------------------- graph and sync

/// Arguments of tools that only take the optional graph.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct GraphArg {
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of `list_graphs`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ListGraphsOut {
    pub graphs: Vec<GraphInfo>,
}

pub(crate) fn get_graph_info(r: &dyn GraphReader, a: GraphArg) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let info = r.graph_info()?;
    let md = format!(
        "# {}\n- path: {}\n- pages: {}\n- blocks: {}\n",
        info.name,
        info.path,
        info.page_count.map_or("?".into(), |n| n.to_string()),
        info.block_count.map_or("?".into(), |n| n.to_string()),
    );
    output(&info, md)
}

pub(crate) fn list_graphs(r: &dyn GraphReader) -> ToolResult {
    let info = r.graph_info()?;
    let md = format!("- {} ({})\n", info.name, info.path);
    output(&ListGraphsOut { graphs: vec![info] }, md)
}

pub(crate) fn sync_status_output(s: &SyncStatus) -> ToolResult {
    let mut md = format!("Sync state: {:?}\n", s.state);
    let _ = writeln!(md, "- ahead: {}, behind: {}", s.ahead, s.behind);
    if s.conflict_count > 0 {
        let _ = writeln!(
            md,
            "- conflicts: {} ({})",
            s.conflict_count,
            s.conflict_pages.join(", ")
        );
    }
    if let Some(e) = &s.last_error {
        let _ = writeln!(md, "- last error: {e}");
    }
    output(s, md)
}
