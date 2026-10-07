//! Block refs, tasks/agenda, namespace tree and graph edges (BIT-SP-0003.R4, R8).

use std::collections::{BTreeSet, HashSet};

use rusqlite::{OptionalExtension, params, params_from_iter, types::Value};

use super::{
    BLOCK_COLS, BlockRow, IndexReader, PAGE_COLS, PageRow, block_from_row, load_properties,
    page_from_row,
};
use crate::Error;

/// Which tasks [`IndexReader::tasks`] returns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskFilter {
    /// Markers to keep (`TODO`, `DOING`, ...); empty = all.
    pub markers: Vec<String>,
    /// Priority `A`/`B`/`C`.
    pub priority: Option<String>,
    /// Only tasks of this page.
    pub page_id: Option<i64>,
}

/// A task block with its page title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    /// The task block.
    pub block: BlockRow,
    /// Display name of its page.
    pub page_name: String,
}

/// Why an item is on the agenda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgendaKind {
    /// `SCHEDULED:` falls in the window.
    Scheduled,
    /// `DEADLINE:` falls in the window.
    Deadline,
    /// A repeating task whose first date is not after the window end.
    Repeated,
}

/// An agenda entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaItem {
    /// The task.
    pub task: TaskItem,
    /// Why it is listed.
    pub kind: AgendaKind,
    /// The date (`yyyyMMdd`) that put it on the agenda.
    pub day: i64,
}

/// A page in the namespace tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceNode {
    /// The page.
    pub page: PageRow,
    /// 0 = the queried page.
    pub depth: usize,
}

/// Options of [`IndexReader::graph_edges`] (defaults follow Logseq's graph view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphOptions {
    /// Show journal pages.
    pub journals: bool,
    /// Show pages without any edge.
    pub orphans: bool,
    /// Show built-in pages (`TODO`, `A`, ...).
    pub builtins: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            journals: false,
            orphans: true,
            builtins: false,
        }
    }
}

/// Edge flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GraphEdgeKind {
    /// A block of `src` references `dst`.
    Ref,
    /// `src` is a namespace child of `dst`.
    Namespace,
}

/// A graph edge between two page ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphEdge {
    /// Source page id.
    pub src: i64,
    /// Destination page id.
    pub dst: i64,
    /// Flavour.
    pub kind: GraphEdgeKind,
}

/// A graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    /// Page id.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Journal page.
    pub is_journal: bool,
    /// Built-in page.
    pub is_builtin: bool,
}

/// Result of [`IndexReader::graph_edges`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphView {
    /// Nodes by name order.
    pub nodes: Vec<GraphNode>,
    /// Edges between visible nodes, sorted.
    pub edges: Vec<GraphEdge>,
}

impl IndexReader {
    /// Number of distinct blocks referencing the block (`((uuid))`, embeds, links).
    pub fn block_ref_count(&self, uuid: &str) -> Result<usize, Error> {
        let conn = self.conn()?;
        let n: i64 = conn.query_row(
            "SELECT count(DISTINCT block_id) FROM block_block_refs WHERE target_uuid = ?1",
            [uuid.to_ascii_lowercase()],
            |r| r.get(0),
        )?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Blocks that reference the block, in page/outline order.
    pub fn block_referrers(&self, uuid: &str) -> Result<Vec<BlockRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&format!(
            "SELECT DISTINCT {BLOCK_COLS} FROM block_block_refs r JOIN blocks b ON b.id = r.block_id \
             WHERE r.target_uuid = ?1 ORDER BY b.page_id, b.file_id, b.ord"
        ))?;
        let mut rows: Vec<BlockRow> = st
            .query_map([uuid.to_ascii_lowercase()], block_from_row)?
            .collect::<Result<_, _>>()?;
        drop(st);
        load_properties(&conn, &mut rows)?;
        Ok(rows)
    }

    /// The block a `((uuid))` points at; `None` when it dangles.
    pub fn resolve_block_ref(&self, uuid: &str) -> Result<Option<BlockRow>, Error> {
        self.block(uuid)
    }

    /// Task blocks (those with a marker), in page/outline order.
    pub fn tasks(&self, filter: &TaskFilter) -> Result<Vec<TaskItem>, Error> {
        let mut sql = format!(
            "SELECT {BLOCK_COLS}, p.original_name FROM blocks b JOIN pages p ON p.id = b.page_id \
             WHERE b.marker IS NOT NULL"
        );
        let mut args: Vec<Value> = Vec::new();
        if !filter.markers.is_empty() {
            let marks = vec!["?"; filter.markers.len()].join(",");
            sql.push_str(&format!(" AND b.marker IN ({marks})"));
            args.extend(filter.markers.iter().map(|m| Value::Text(m.clone())));
        }
        if let Some(p) = &filter.priority {
            sql.push_str(" AND b.priority = ?");
            args.push(Value::Text(p.clone()));
        }
        if let Some(id) = filter.page_id {
            sql.push_str(" AND b.page_id = ?");
            args.push(Value::Integer(id));
        }
        sql.push_str(" ORDER BY p.name, b.file_id, b.ord");
        self.task_items(&sql, args)
    }

    fn task_items(&self, sql: &str, args: Vec<Value>) -> Result<Vec<TaskItem>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare(sql)?;
        let mut rows: Vec<(BlockRow, String)> = st
            .query_map(params_from_iter(args), |r| {
                Ok((block_from_row(r)?, r.get::<_, String>(20)?))
            })?
            .collect::<Result<_, _>>()?;
        drop(st);
        let mut blocks: Vec<BlockRow> = rows.iter().map(|(b, _)| b.clone()).collect();
        load_properties(&conn, &mut blocks)?;
        for ((slot, _), b) in rows.iter_mut().zip(blocks) {
            *slot = b;
        }
        Ok(rows
            .into_iter()
            .map(|(block, page_name)| TaskItem { block, page_name })
            .collect())
    }

    /// Open tasks scheduled or due in `[today, today + days_ahead]` (`yyyyMMdd`), plus repeating
    /// tasks whose first date is not after the window end; `DONE`/`CANCELED`/`CANCELLED` are
    /// excluded. Sorted by date, then page.
    pub fn agenda(&self, today: i64, days_ahead: u32) -> Result<Vec<AgendaItem>, Error> {
        let future: i64 = {
            let conn = self.conn()?;
            let t = today.to_string();
            let date = format!(
                "{}-{}-{}",
                t.get(0..4).unwrap_or("0000"),
                t.get(4..6).unwrap_or("00"),
                t.get(6..8).unwrap_or("00")
            );
            conn.query_row(
                "SELECT CAST(strftime('%Y%m%d', ?1, '+' || ?2 || ' days') AS INTEGER)",
                params![date, days_ahead],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .unwrap_or(today)
        };
        let sql = format!(
            "SELECT {BLOCK_COLS}, p.original_name FROM blocks b JOIN pages p ON p.id = b.page_id \
             WHERE b.marker IS NOT NULL AND b.marker NOT IN ('DONE','CANCELED','CANCELLED') \
             AND ((b.scheduled BETWEEN ?1 AND ?2) OR (b.deadline BETWEEN ?1 AND ?2) \
                  OR (b.repeated = 1 AND COALESCE(b.scheduled, b.deadline) <= ?2))"
        );
        let items = self.task_items(&sql, vec![Value::Integer(today), Value::Integer(future)])?;
        let mut out: Vec<AgendaItem> = items
            .into_iter()
            .filter_map(|task| {
                let in_window = |d: Option<i64>| d.filter(|d| (today..=future).contains(d));
                let (kind, day) = if let Some(d) = in_window(task.block.scheduled) {
                    (AgendaKind::Scheduled, d)
                } else if let Some(d) = in_window(task.block.deadline) {
                    (AgendaKind::Deadline, d)
                } else {
                    let d = task.block.scheduled.or(task.block.deadline)?;
                    (AgendaKind::Repeated, d)
                };
                Some(AgendaItem { task, kind, day })
            })
            .collect();
        out.sort_by(|a, b| {
            a.day
                .cmp(&b.day)
                .then_with(|| a.task.page_name.cmp(&b.task.page_name))
                .then_with(|| a.task.block.ord.cmp(&b.task.block.ord))
        });
        Ok(out)
    }

    /// Direct namespace children of a page, by name.
    pub fn namespace_children(&self, page_id: i64) -> Result<Vec<PageRow>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&format!(
            "SELECT {PAGE_COLS} FROM pages p WHERE p.namespace_parent_id = ?1 ORDER BY p.name"
        ))?;
        let rows = st
            .query_map([page_id], page_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// The page and all its namespace descendants in pre-order (siblings by name).
    pub fn namespace_tree(&self, page_id: i64) -> Result<Vec<NamespaceNode>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare_cached(&format!(
            "WITH RECURSIVE ns(id, depth, path) AS ( \
               SELECT id, 0, name FROM pages WHERE id = ?1 \
               UNION ALL \
               SELECT p.id, ns.depth + 1, ns.path || char(31) || p.name \
               FROM pages p JOIN ns ON p.namespace_parent_id = ns.id WHERE ns.depth < 32) \
             SELECT {PAGE_COLS}, ns.depth FROM ns JOIN pages p ON p.id = ns.id ORDER BY ns.path"
        ))?;
        let rows = st
            .query_map([page_id], |r| {
                Ok(NamespaceNode {
                    page: page_from_row(r)?,
                    depth: usize::try_from(r.get::<_, i64>(11)?).unwrap_or(0),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Pages and edges for the graph view: direct page references plus namespace edges, minus
    /// pages with `exclude-from-graph-view:: true` and whatever `opts` hides.
    pub fn graph_edges(&self, opts: GraphOptions) -> Result<GraphView, Error> {
        let conn = self.conn()?;
        let mut nodes: Vec<GraphNode> = Vec::new();
        {
            let mut st = conn.prepare(
                "SELECT p.id, p.original_name, p.is_journal, p.is_builtin FROM pages p \
                 WHERE NOT EXISTS (SELECT 1 FROM page_property_values v WHERE v.page_id = p.id \
                     AND v.key = 'exclude-from-graph-view' AND v.value_num = 1) \
                 ORDER BY p.name",
            )?;
            let it = st.query_map([], |r| {
                Ok(GraphNode {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    is_journal: r.get(2)?,
                    is_builtin: r.get(3)?,
                })
            })?;
            for n in it {
                let n = n?;
                if (n.is_journal && !opts.journals) || (n.is_builtin && !opts.builtins) {
                    continue;
                }
                nodes.push(n);
            }
        }
        let visible: HashSet<i64> = nodes.iter().map(|n| n.id).collect();
        let mut edges: BTreeSet<GraphEdge> = BTreeSet::new();
        {
            let mut st = conn.prepare(
                "SELECT DISTINCT b.page_id, r.page_id FROM block_page_refs r \
                 JOIN blocks b ON b.id = r.block_id WHERE b.page_id <> r.page_id",
            )?;
            for e in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
                let (src, dst) = e?;
                if visible.contains(&src) && visible.contains(&dst) {
                    edges.insert(GraphEdge {
                        src,
                        dst,
                        kind: GraphEdgeKind::Ref,
                    });
                }
            }
            let mut st = conn.prepare(
                "SELECT id, namespace_parent_id FROM pages WHERE namespace_parent_id IS NOT NULL",
            )?;
            for e in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
                let (src, dst) = e?;
                if visible.contains(&src) && visible.contains(&dst) {
                    edges.insert(GraphEdge {
                        src,
                        dst,
                        kind: GraphEdgeKind::Namespace,
                    });
                }
            }
        }
        if !opts.orphans {
            let connected: HashSet<i64> = edges.iter().flat_map(|e| [e.src, e.dst]).collect();
            nodes.retain(|n| connected.contains(&n.id));
        }
        Ok(GraphView {
            nodes,
            edges: edges.into_iter().collect(),
        })
    }

    /// Placeholder lookup helper for callers resolving a page title to an id.
    pub fn page_id(&self, name: &str) -> Result<Option<i64>, Error> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT id FROM pages WHERE name = ?1",
                [bitacora_core::naming::page_key(name)],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Ranked search over pages and blocks (see [`crate::search`]) on a pooled reader.
    pub fn search(
        &self,
        input: &str,
        opts: &crate::search::SearchOptions,
    ) -> Result<Vec<crate::search::SearchHit>, Error> {
        let conn = self.conn()?;
        crate::search::search_cached(&conn, input, opts, Some(self.pool.titles()))
    }

    /// Number of distinct blocks that reference each page (`pages.id` to count), for the
    /// "backlinks" column of the all-pages table.
    pub fn backlink_counts(&self) -> Result<std::collections::HashMap<i64, usize>, Error> {
        let conn = self.conn()?;
        let mut st = conn.prepare(
            "SELECT page_id, COUNT(DISTINCT block_id) FROM block_page_refs GROUP BY page_id",
        )?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, n)| (id, usize::try_from(n).unwrap_or(0)))
            .collect())
    }
}
