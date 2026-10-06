//! Running a block's live query against the index and shaping the result (BIT-US-0102).
//!
//! Everything here is GPUI-free and blocking: views call [`run`] from a background task.
//! [`table`] turns results into sortable columns.

pub mod table;

use bitacora_core::date::Date;
use bitacora_index::query::{Cell, QueryContext, QueryError};
use bitacora_index::{BlockRow, IndexReader, PageRow};

use crate::render::widget::{QueryKind, QuerySpec};

/// Most rows a query shows (the index is asked for no more).
pub const RESULT_CAP: usize = 500;

/// Where the query runs (`:current-page`, the block excluded from its own results).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// Title of the page the query block is on.
    pub page: Option<String>,
    /// UUID of the query block, when it has one.
    pub block: Option<String>,
}

/// The shaped result of a query.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Matching blocks.
    Blocks(Vec<BlockRow>),
    /// Matching pages.
    Pages(Vec<PageRow>),
    /// Anything else (aggregates, scalars, mixed tuples).
    Rows {
        /// Column labels.
        columns: Vec<String>,
        /// Tuples.
        rows: Vec<Vec<Cell>>,
    },
}

impl Body {
    /// Number of results.
    pub fn len(&self) -> usize {
        match self {
            Self::Blocks(b) => b.len(),
            Self::Pages(p) => p.len(),
            Self::Rows { rows, .. } => rows.len(),
        }
    }

    /// No result.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A finished query.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// `:title` of an advanced query.
    pub title: Option<String>,
    /// `:collapsed?` of an advanced query.
    pub collapsed: bool,
    /// The results.
    pub body: Body,
    /// Constructs that were ignored (`unsupported: :result-transform`).
    pub warnings: Vec<String>,
    /// The query returned more than [`RESULT_CAP`] rows; the rest is not shown.
    pub truncated: bool,
}

/// Why a query did not produce results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The query text is malformed.
    Syntax(String),
    /// The query needs something the engine does not support.
    Unsupported(String),
    /// The index failed.
    Index(String),
}

impl Failure {
    /// The message to show.
    pub fn message(&self) -> &str {
        match self {
            Self::Syntax(m) | Self::Unsupported(m) | Self::Index(m) => m,
        }
    }
}

impl From<QueryError> for Failure {
    fn from(e: QueryError) -> Self {
        match e {
            QueryError::Syntax(m) => Self::Syntax(m),
            QueryError::Unsupported(c) => Self::Unsupported(c),
            QueryError::Index(e) => Self::Index(e.to_string()),
        }
    }
}

/// The query context for the local clock and `scope`.
pub fn context(reader: &IndexReader, scope: &Scope, today: Date, now_ms: i64) -> QueryContext {
    let mut ctx = QueryContext::new(today, now_ms);
    if let Some(ms) = local_midnight_ms() {
        ctx.today_start_ms = ms;
    }
    ctx.limit = Some(RESULT_CAP + 1);
    ctx.current_block = scope.block.clone();
    ctx.query_block = scope.block.clone();
    ctx.current_page = scope.page.as_deref().map(str::trim).and_then(|name| {
        if name.is_empty() {
            return None;
        }
        Some(match reader.page_by_name(name) {
            Ok(Some(p)) => p.name,
            _ => name.to_lowercase(),
        })
    });
    ctx
}

fn local_midnight_ms() -> Option<i64> {
    let now = jiff::Zoned::now();
    let start = now.start_of_day().ok()?;
    Some(start.timestamp().as_millisecond())
}

/// Runs `spec` and shapes the result.
pub fn run(
    reader: &IndexReader,
    spec: &QuerySpec,
    scope: &Scope,
    today: Date,
    now_ms: i64,
) -> Result<Output, Failure> {
    let ctx = context(reader, scope, today, now_ms);
    match spec.kind {
        QueryKind::Simple => {
            let res = reader.query_simple(&spec.source, &ctx)?;
            let mut body = match res.kind {
                bitacora_index::query::ResultKind::Blocks => Body::Blocks(res.blocks),
                bitacora_index::query::ResultKind::Pages => Body::Pages(res.pages),
            };
            let truncated = cap(&mut body);
            Ok(Output {
                title: None,
                collapsed: false,
                body,
                warnings: Vec::new(),
                truncated,
            })
        }
        QueryKind::Advanced => {
            let out = reader.query_advanced(&spec.source, &ctx)?;
            let mut body = shape_rows(out.columns.clone(), out.rows.clone());
            let truncated = cap(&mut body);
            Ok(Output {
                title: out.title.clone(),
                collapsed: out.collapsed,
                body,
                warnings: out.warnings.iter().map(ToString::to_string).collect(),
                truncated,
            })
        }
    }
}

fn cap(body: &mut Body) -> bool {
    let over = body.len() > RESULT_CAP;
    if over {
        match body {
            Body::Blocks(b) => b.truncate(RESULT_CAP),
            Body::Pages(p) => p.truncate(RESULT_CAP),
            Body::Rows { rows, .. } => rows.truncate(RESULT_CAP),
        }
    }
    over
}

/// Blocks-only or pages-only single-column results become block / page lists; everything else
/// stays a table of cells.
pub fn shape_rows(columns: Vec<String>, rows: Vec<Vec<Cell>>) -> Body {
    if rows.is_empty() {
        return Body::Blocks(Vec::new());
    }
    let single = |f: &dyn Fn(&Cell) -> bool| rows.iter().all(|r| r.len() == 1 && f(&r[0]));
    if single(&|c| matches!(c, Cell::Block(_))) {
        return Body::Blocks(
            rows.into_iter()
                .filter_map(|mut r| match r.pop() {
                    Some(Cell::Block(b)) => Some(*b),
                    _ => None,
                })
                .collect(),
        );
    }
    if single(&|c| matches!(c, Cell::Page(_))) {
        return Body::Pages(
            rows.into_iter()
                .filter_map(|mut r| match r.pop() {
                    Some(Cell::Page(p)) => Some(*p),
                    _ => None,
                })
                .collect(),
        );
    }
    Body::Rows { columns, rows }
}

/// Text of a cell in a table.
pub fn cell_text(c: &Cell) -> String {
    match c {
        Cell::Null => String::new(),
        Cell::Int(n) => n.to_string(),
        Cell::Real(f) => f.to_string(),
        Cell::Text(t) => t.clone(),
        Cell::Block(b) => b.title.clone(),
        Cell::Page(p) => p.original_name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn block_row(uuid: &str) -> BlockRow {
        BlockRow {
            id: 1,
            uuid: uuid.into(),
            page_id: 1,
            file_id: 1,
            parent_id: None,
            ord: 0,
            subtree_end: 0,
            depth: 1,
            is_pre_block: false,
            content: uuid.into(),
            title: uuid.into(),
            marker: None,
            priority: None,
            scheduled: None,
            deadline: None,
            collapsed: false,
            heading: None,
            byte_start: 0,
            byte_end: 0,
            line_start: 1,
            properties: Vec::new(),
        }
    }

    fn block(uuid: &str) -> Cell {
        Cell::Block(Box::new(block_row(uuid)))
    }

    #[test]
    fn single_block_column_becomes_a_block_list() {
        let body = shape_rows(vec!["?b".into()], vec![vec![block("a")], vec![block("b")]]);
        assert!(matches!(&body, Body::Blocks(b) if b.len() == 2));
    }

    #[test]
    fn aggregates_stay_rows_and_empty_is_an_empty_block_list() {
        let body = shape_rows(vec!["(count ?b)".into()], vec![vec![Cell::Int(3)]]);
        assert!(matches!(body, Body::Rows { .. }));
        assert!(shape_rows(vec![], vec![]).is_empty());
        let mixed = shape_rows(
            vec!["a".into(), "b".into()],
            vec![vec![block("a"), Cell::Int(1)]],
        );
        assert!(matches!(mixed, Body::Rows { .. }));
    }

    #[test]
    fn cells_have_text() {
        assert_eq!(cell_text(&Cell::Int(4)), "4");
        assert_eq!(cell_text(&Cell::Null), "");
        assert_eq!(cell_text(&block("x")), "x");
    }
}
