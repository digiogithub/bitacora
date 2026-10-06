//! Table model of query results: columns, visible-column selection and sorting, following the
//! `query-properties::`, `query-sort-by::` and `query-sort-desc::` block properties.

use std::cmp::Ordering;
use std::collections::HashMap;

use bitacora_index::query::Cell;
use bitacora_index::{BlockRow, PageRow};
use bitacora_markdown::properties::normalize_key;

use super::{Body, cell_text};
use crate::render::model::is_hidden_property;
use crate::render::widget::QueryProps;

/// The title column of block tables.
pub const COL_BLOCK: &str = "block";
/// The page column of block tables.
pub const COL_PAGE: &str = "page";

/// Block fields that are columns although they are not properties.
const BLOCK_FIELDS: &[&str] = &["marker", "priority", "scheduled", "deadline"];

/// Where a click on a row goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowTarget {
    /// A block (UUID).
    Block(String),
    /// A page (title).
    Page(String),
    /// Nowhere.
    None,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// Cell text per column of [`Table::columns`].
    pub cells: Vec<String>,
    /// What the first cell opens.
    pub target: RowTarget,
    /// Page title of the row (clickable page column), when known.
    pub page: Option<String>,
}

/// A table ready to draw.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    /// Visible column keys (normalised), in order.
    pub columns: Vec<String>,
    /// Every column that could be shown (the column picker lists these).
    pub available: Vec<String>,
    /// Rows in sorted order.
    pub rows: Vec<TableRow>,
}

/// Block and page names for the page column.
pub type PageNames = HashMap<i64, String>;

/// The default visible columns: block, page and every property.
fn default_columns(available: &[String]) -> Vec<String> {
    available
        .iter()
        .filter(|c| !BLOCK_FIELDS.contains(&c.as_str()))
        .cloned()
        .collect()
}

fn block_available(blocks: &[BlockRow]) -> Vec<String> {
    let mut out = vec![COL_BLOCK.to_owned(), COL_PAGE.to_owned()];
    for b in blocks {
        for (k, _) in &b.properties {
            let key = normalize_key(k);
            if is_hidden_property(&key) || out.contains(&key) {
                continue;
            }
            out.push(key);
        }
    }
    for f in BLOCK_FIELDS {
        if blocks.iter().any(|b| field_value(b, f).is_some()) && !out.iter().any(|c| c == f) {
            out.push((*f).to_owned());
        }
    }
    out
}

fn field_value(b: &BlockRow, field: &str) -> Option<String> {
    match field {
        "marker" => b.marker.clone(),
        "priority" => b.priority.clone(),
        "scheduled" => b.scheduled.map(day_text),
        "deadline" => b.deadline.map(day_text),
        _ => None,
    }
}

/// `20260315` as `2026-03-15`.
fn day_text(d: i64) -> String {
    format!("{:04}-{:02}-{:02}", d / 10_000, (d / 100) % 100, d % 100)
}

fn block_cell(b: &BlockRow, col: &str, pages: &PageNames) -> String {
    match col {
        COL_BLOCK => b.title.clone(),
        COL_PAGE => pages.get(&b.page_id).cloned().unwrap_or_default(),
        _ => {
            if let Some((_, v)) = b.properties.iter().find(|(k, _)| normalize_key(k) == col) {
                return v.trim().to_owned();
            }
            field_value(b, col).unwrap_or_default()
        }
    }
}

/// The table of a block result.
pub fn block_table(blocks: &[BlockRow], pages: &PageNames, props: &QueryProps) -> Table {
    let available = block_available(blocks);
    let columns = visible(&available, props, default_columns(&available));
    let rows = blocks
        .iter()
        .map(|b| TableRow {
            cells: columns.iter().map(|c| block_cell(b, c, pages)).collect(),
            target: RowTarget::Block(b.uuid.clone()),
            page: pages.get(&b.page_id).cloned(),
        })
        .collect();
    finish(columns, available, rows, props)
}

/// The table of a page result: name and times.
pub fn page_table(pages: &[PageRow], props: &QueryProps) -> Table {
    let available: Vec<String> = [COL_PAGE, "created-at", "updated-at"]
        .map(str::to_owned)
        .to_vec();
    let columns = visible(&available, props, available.clone());
    let rows = pages
        .iter()
        .map(|p| TableRow {
            cells: columns
                .iter()
                .map(|c| match c.as_str() {
                    COL_PAGE => p.original_name.clone(),
                    "created-at" => p.created_at.map(time_text).unwrap_or_default(),
                    _ => p.updated_at.map(time_text).unwrap_or_default(),
                })
                .collect(),
            target: RowTarget::Page(p.original_name.clone()),
            page: Some(p.original_name.clone()),
        })
        .collect();
    finish(columns, available, rows, props)
}

fn time_text(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// The table of aggregate / scalar rows.
pub fn cell_table(columns: &[String], rows: &[Vec<Cell>], props: &QueryProps) -> Table {
    let available: Vec<String> = columns.to_vec();
    let cols = visible(&available, props, available.clone());
    let index: Vec<usize> = cols
        .iter()
        .filter_map(|c| available.iter().position(|a| a == c))
        .collect();
    let rows = rows
        .iter()
        .map(|r| TableRow {
            cells: index
                .iter()
                .map(|&i| r.get(i).map(cell_text).unwrap_or_default())
                .collect(),
            target: match r.first() {
                Some(Cell::Block(b)) => RowTarget::Block(b.uuid.clone()),
                Some(Cell::Page(p)) => RowTarget::Page(p.original_name.clone()),
                _ => RowTarget::None,
            },
            page: None,
        })
        .collect();
    finish(cols, available, rows, props)
}

/// The table for any [`Body`].
pub fn table_of(body: &Body, pages: &PageNames, props: &QueryProps) -> Table {
    match body {
        Body::Blocks(b) => block_table(b, pages, props),
        Body::Pages(p) => page_table(p, props),
        Body::Rows { columns, rows } => cell_table(columns, rows, props),
    }
}

/// `query-properties::` picks and orders the columns (unknown names are ignored); without it
/// the defaults show.
fn visible(available: &[String], props: &QueryProps, default: Vec<String>) -> Vec<String> {
    match &props.properties {
        Some(list) => {
            let mut out: Vec<String> = Vec::new();
            for name in list {
                let n = normalize_key(name);
                if available.contains(&n) && !out.contains(&n) {
                    out.push(n);
                }
            }
            if out.is_empty() { default } else { out }
        }
        None => default,
    }
}

fn finish(
    columns: Vec<String>,
    available: Vec<String>,
    mut rows: Vec<TableRow>,
    props: &QueryProps,
) -> Table {
    if let Some(by) = &props.sort_by {
        sort_rows(
            &columns,
            &mut rows,
            &normalize_key(by),
            props.sort_desc.unwrap_or(false),
        );
    }
    Table {
        columns,
        available,
        rows,
    }
}

/// Sorts `rows` by column `by` (no-op when it is not shown). Numbers compare numerically,
/// text case-insensitively; empty cells always go last.
pub fn sort_rows(columns: &[String], rows: &mut [TableRow], by: &str, desc: bool) {
    let Some(ix) = columns.iter().position(|c| c == by) else {
        return;
    };
    rows.sort_by(|a, b| {
        let (x, y) = (a.cells[ix].trim(), b.cells[ix].trim());
        match (x.is_empty(), y.is_empty()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => {}
        }
        let ord = match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(p), Ok(q)) => p.partial_cmp(&q).unwrap_or(Ordering::Equal),
            _ => x.to_lowercase().cmp(&y.to_lowercase()),
        };
        if desc { ord.reverse() } else { ord }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::query::tests::block_row;

    fn b(uuid: &str, page: i64, title: &str, props: &[(&str, &str)]) -> BlockRow {
        let mut r = block_row(uuid);
        r.page_id = page;
        r.title = title.into();
        r.properties = props
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        r
    }

    fn names() -> PageNames {
        HashMap::from([(1, "Alpha".to_owned()), (2, "Beta".to_owned())])
    }

    #[test]
    fn default_columns_are_block_page_and_properties_without_hidden_ones() {
        let blocks = vec![
            b(
                "a",
                1,
                "one",
                &[("status", "open"), ("id", "x"), ("collapsed", "true")],
            ),
            b("b", 2, "two", &[("Due-Date", "2026")]),
        ];
        let t = block_table(&blocks, &names(), &QueryProps::default());
        assert_eq!(t.columns, ["block", "page", "status", "due-date"]);
        assert_eq!(t.rows[0].cells, ["one", "Alpha", "open", ""]);
        assert_eq!(t.rows[1].cells, ["two", "Beta", "", "2026"]);
        assert_eq!(t.rows[0].target, RowTarget::Block("a".into()));
    }

    #[test]
    fn query_properties_choose_and_order_columns() {
        let blocks = vec![b("a", 1, "one", &[("status", "open")])];
        let props = QueryProps {
            properties: Some(vec!["status".into(), "block".into(), "nope".into()]),
            ..QueryProps::default()
        };
        let t = block_table(&blocks, &names(), &props);
        assert_eq!(t.columns, ["status", "block"]);
        assert!(t.available.contains(&"page".to_owned()));
    }

    #[test]
    fn sorting_is_numeric_then_textual_and_blanks_go_last() {
        let blocks = vec![
            b("a", 1, "x", &[("n", "10")]),
            b("b", 1, "y", &[("n", "9")]),
            b("c", 1, "z", &[]),
            b("d", 1, "w", &[("n", "100")]),
        ];
        let mut props = QueryProps {
            sort_by: Some("n".into()),
            ..QueryProps::default()
        };
        let t = block_table(&blocks, &names(), &props);
        let order: Vec<&str> = t.rows.iter().map(|r| r.cells[0].as_str()).collect();
        assert_eq!(order, ["y", "x", "w", "z"]);
        props.sort_desc = Some(true);
        let t = block_table(&blocks, &names(), &props);
        let order: Vec<&str> = t.rows.iter().map(|r| r.cells[0].as_str()).collect();
        assert_eq!(order, ["w", "x", "y", "z"]);
    }

    #[test]
    fn marker_and_priority_are_available_columns() {
        let mut r = b("a", 1, "t", &[]);
        r.marker = Some("TODO".into());
        r.priority = Some("A".into());
        r.deadline = Some(20260315);
        let props = QueryProps {
            properties: Some(vec!["block".into(), "marker".into(), "deadline".into()]),
            ..QueryProps::default()
        };
        let t = block_table(&[r], &names(), &props);
        assert_eq!(t.rows[0].cells, ["t", "TODO", "2026-03-15"]);
    }

    #[test]
    fn cell_tables_keep_the_column_order() {
        let cols = vec!["?name".to_owned(), "(count ?b)".to_owned()];
        let rows = vec![
            vec![Cell::Text("b".into()), Cell::Int(2)],
            vec![Cell::Text("a".into()), Cell::Int(5)],
        ];
        let props = QueryProps {
            sort_by: Some("?name".into()),
            ..QueryProps::default()
        };
        let t = cell_table(&cols, &rows, &props);
        assert_eq!(t.rows[0].cells, ["a", "5"]);
    }
}
