//! Graph-view data (BIT-SP-0012.R1/R2): pages as nodes, references / tags / namespaces as edges,
//! with Logseq-style filters, for the whole graph and for one page's neighbourhood.
//!
//! The query is read-only and fully derived from the index tables, so a rebuild from scratch
//! yields the same [`GraphData`] as the incremental path.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::Error;
use crate::read::IndexReader;

/// `block_page_refs.kind` values that count as graph edges: link, tag, property value, embed.
const REF_KINDS: &str = "1, 2, 3, 8";

/// Filters of the graph view (defaults follow Logseq: journals off, orphans on, builtins off).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphFilter {
    /// Show journal pages.
    pub journals: bool,
    /// Show pages without any edge (after the other filters).
    pub orphans: bool,
    /// Show built-in pages (`TODO`, `A`, ...).
    pub builtins: bool,
    /// Page names hidden from the graph (the `:graph/settings` excluded pages), any case.
    pub excluded_pages: Vec<String>,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            journals: false,
            orphans: true,
            builtins: false,
            excluded_pages: Vec::new(),
        }
    }
}

/// A node of the graph view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphDataNode {
    /// Page id.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Journal page.
    pub is_journal: bool,
    /// The page is used as a tag (`#tag` or `tags::`).
    pub is_tag: bool,
    /// Some page is a namespace child of this page.
    pub is_namespace_parent: bool,
    /// Number of edges incident to the node among the visible nodes.
    pub degree: u32,
}

/// A directed edge between two page ids (`src` references / tags / is a child of `dst`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphDataEdge {
    /// Source page id.
    pub src: i64,
    /// Destination page id.
    pub dst: i64,
}

/// Nodes (sorted by normalised page name) and edges (sorted, deduplicated, no self-links).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphData {
    /// Visible nodes.
    pub nodes: Vec<GraphDataNode>,
    /// Edges between visible nodes.
    pub edges: Vec<GraphDataEdge>,
}

struct Raw {
    id: i64,
    name: String,
    original: String,
    is_journal: bool,
    is_builtin: bool,
    hidden: bool,
}

/// UUID-shaped page names (block-ref placeholders) are not graph nodes.
fn is_uuid_like(name: &str) -> bool {
    uuid::Uuid::parse_str(name.trim()).is_ok()
}

/// Asset links (`../assets/x.png`) create placeholder pages that are not graph nodes.
fn is_asset_name(name: &str) -> bool {
    let n = name.trim_start_matches("../").trim_start_matches("./");
    n.starts_with("assets/") || n.starts_with("assets\\")
}

impl IndexReader {
    /// The whole graph, filtered by `filter`.
    pub fn graph_data(&self, filter: &GraphFilter) -> Result<GraphData, Error> {
        let (raws, edges, tags, parents) = self.load_graph()?;
        Ok(finish(&raws, &edges, &tags, &parents, filter, None))
    }

    /// Local graph of one page: the page, its 1-hop neighbours (in either direction) and the
    /// links among them. Journals follow `filter.journals`; the centre is always present.
    pub fn local_graph_data(&self, page_id: i64, filter: &GraphFilter) -> Result<GraphData, Error> {
        let (raws, edges, tags, parents) = self.load_graph()?;
        Ok(finish(
            &raws,
            &edges,
            &tags,
            &parents,
            filter,
            Some(page_id),
        ))
    }

    #[allow(clippy::type_complexity)]
    fn load_graph(
        &self,
    ) -> Result<
        (
            Vec<Raw>,
            BTreeSet<GraphDataEdge>,
            HashSet<i64>,
            HashSet<i64>,
        ),
        Error,
    > {
        let conn = self.conn()?;
        let mut raws = Vec::new();
        {
            let mut st = conn.prepare(
                "SELECT p.id, p.name, p.original_name, p.is_journal, p.is_builtin, \
                   EXISTS (SELECT 1 FROM page_property_values v WHERE v.page_id = p.id \
                     AND v.key = 'exclude-from-graph-view' AND v.value_num = 1) \
                 FROM pages p ORDER BY p.name",
            )?;
            let it = st.query_map([], |r| {
                Ok(Raw {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    original: r.get(2)?,
                    is_journal: r.get(3)?,
                    is_builtin: r.get(4)?,
                    hidden: r.get(5)?,
                })
            })?;
            for r in it {
                raws.push(r?);
            }
        }
        let mut edges = BTreeSet::new();
        let mut tags = HashSet::new();
        let mut parents = HashSet::new();
        let mut add = |src: i64, dst: i64| {
            if src != dst {
                edges.insert(GraphDataEdge { src, dst });
            }
        };
        let mut st = conn.prepare(&format!(
            "SELECT DISTINCT b.page_id, r.page_id, r.kind FROM block_page_refs r \
             JOIN blocks b ON b.id = r.block_id WHERE r.kind IN ({REF_KINDS})"
        ))?;
        for e in st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (src, dst, kind) = e?;
            if kind == 2 {
                tags.insert(dst);
            }
            add(src, dst);
        }
        let mut st = conn.prepare("SELECT page_id, tag_page_id FROM page_tags")?;
        for e in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (src, dst) = e?;
            tags.insert(dst);
            add(src, dst);
        }
        let mut st = conn.prepare(
            "SELECT id, namespace_parent_id FROM pages WHERE namespace_parent_id IS NOT NULL",
        )?;
        for e in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (src, dst) = e?;
            parents.insert(dst);
            add(src, dst);
        }
        Ok((raws, edges, tags, parents))
    }
}

fn finish(
    raws: &[Raw],
    all_edges: &BTreeSet<GraphDataEdge>,
    tags: &HashSet<i64>,
    parents: &HashSet<i64>,
    filter: &GraphFilter,
    centre: Option<i64>,
) -> GraphData {
    let excluded: HashSet<String> = filter
        .excluded_pages
        .iter()
        .map(|n| n.trim().to_lowercase())
        .collect();
    let mut visible: HashMap<i64, &Raw> = raws
        .iter()
        .filter(|p| {
            Some(p.id) == centre
                || (!p.hidden
                    && !excluded.contains(&p.name)
                    && !is_uuid_like(&p.name)
                    && !is_asset_name(&p.name)
                    && (filter.journals || !p.is_journal)
                    && (filter.builtins || !p.is_builtin))
        })
        .map(|p| (p.id, p))
        .collect();
    let mut edges: BTreeSet<GraphDataEdge> = all_edges
        .iter()
        .filter(|e| visible.contains_key(&e.src) && visible.contains_key(&e.dst))
        .copied()
        .collect();
    if let Some(c) = centre {
        let mut keep: HashSet<i64> = HashSet::from([c]);
        for e in &edges {
            if e.src == c {
                keep.insert(e.dst);
            } else if e.dst == c {
                keep.insert(e.src);
            }
        }
        visible.retain(|id, _| keep.contains(id));
        edges.retain(|e| keep.contains(&e.src) && keep.contains(&e.dst));
    } else if !filter.orphans {
        let connected: HashSet<i64> = edges.iter().flat_map(|e| [e.src, e.dst]).collect();
        visible.retain(|id, _| connected.contains(id));
    }
    let mut degree: HashMap<i64, u32> = HashMap::new();
    for e in &edges {
        *degree.entry(e.src).or_default() += 1;
        *degree.entry(e.dst).or_default() += 1;
    }
    // `raws` is already ordered by normalised name; keep that order for determinism.
    let nodes = raws
        .iter()
        .filter(|p| visible.contains_key(&p.id))
        .map(|p| GraphDataNode {
            id: p.id,
            name: p.original.clone(),
            is_journal: p.is_journal,
            is_tag: tags.contains(&p.id),
            is_namespace_parent: parents.contains(&p.id),
            degree: degree.get(&p.id).copied().unwrap_or(0),
        })
        .collect();
    GraphData {
        nodes,
        edges: edges.into_iter().collect(),
    }
}
