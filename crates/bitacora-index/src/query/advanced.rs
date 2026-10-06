//! Advanced queries: `#+BEGIN_QUERY ... #+END_QUERY` blocks (BIT-US-0103, design §8).
//!
//! The block holds an EDN map (`:title`, `:query`, `:inputs`, `:collapsed?`, `:result-transform`,
//! `:view`, ...). A `:query` that is a Datalog vector (`[:find ... :where ...]`) is compiled to SQL
//! (see [`super::datalog`]); a list or string `:query` is a simple DSL query. Constructs outside
//! the subset make the query fail with `unsupported: <construct>`, except `:result-transform`,
//! `:view`, user `:rules` and exotic `pull` patterns: those are reported in
//! [`AdvancedOutcome::warnings`] and the raw rows are still returned.

use std::collections::HashMap;

use rusqlite::functions::FunctionFlags;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params_from_iter};

use super::datalog::{ColKind, Comp, assemble};
use super::edn::{Edn, read_all};
use super::{QueryContext, QueryError, ResultKind};
use crate::read::{
    BLOCK_COLS, BlockRow, PAGE_COLS, PageRow, block_from_row, load_properties, page_from_row,
};

pub use super::datalog::Shape;

/// A construct that was left out; the rest of the query still ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// What was ignored (`:result-transform`, `:view`, ...).
    pub construct: String,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported: {}", self.construct)
    }
}

/// One value of a result row.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// No value.
    Null,
    /// Integer (counts, journal days, timestamps, booleans as 0/1).
    Int(i64),
    /// Real number.
    Real(f64),
    /// Text.
    Text(String),
    /// A block entity (`?b`, `(pull ?b [*])`).
    Block(Box<BlockRow>),
    /// A page entity.
    Page(Box<PageRow>),
}

/// Result of an advanced query.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedOutcome {
    /// `:title`.
    pub title: Option<String>,
    /// `:collapsed?`.
    pub collapsed: bool,
    /// Shape of the `:find` spec.
    pub shape: Shape,
    /// One name per column (`?b`, `(count ?b)`).
    pub columns: Vec<String>,
    /// Result tuples.
    pub rows: Vec<Vec<Cell>>,
    /// Ignored constructs, as `unsupported: <construct>`.
    pub warnings: Vec<Unsupported>,
    /// The generated SQL (Datalog queries), for diagnostics.
    pub sql: Option<String>,
}

impl AdvancedOutcome {
    /// Every block in the rows, in order.
    #[must_use]
    pub fn blocks(&self) -> Vec<&BlockRow> {
        self.rows
            .iter()
            .flatten()
            .filter_map(|c| match c {
                Cell::Block(b) => Some(&**b),
                _ => None,
            })
            .collect()
    }

    /// Every page in the rows, in order.
    #[must_use]
    pub fn pages(&self) -> Vec<&PageRow> {
        self.rows
            .iter()
            .flatten()
            .filter_map(|c| match c {
                Cell::Page(p) => Some(&**p),
                _ => None,
            })
            .collect()
    }
}

/// Text between `#+BEGIN_QUERY` and `#+END_QUERY` (or the whole text when there are no markers).
fn strip_markers(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
        let t = line.trim().to_ascii_uppercase();
        if t.starts_with("#+BEGIN_QUERY") || t.starts_with("#+END_QUERY") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Keys that only affect rendering and need no warning.
const COSMETIC_KEYS: &[&str] = &[
    "title",
    "query",
    "inputs",
    "collapsed?",
    "group-by-page?",
    "breadcrumb-show?",
    "table-view?",
    "children?",
    "rules",
    "result-transform",
    "view",
];

/// Parses and runs an advanced query.
pub fn run(
    conn: &Connection,
    src: &str,
    ctx: &QueryContext,
) -> Result<AdvancedOutcome, QueryError> {
    let text = strip_markers(src);
    let transformed = super::dsl::pre_transform(&text);
    // A DSL `:query` uses `[[page]]` and `#tag`, which need the DSL pre-transform; Datalog
    // vectors must not get it (`[[?a ?b]]` is a legitimate nested vector there).
    let dsl_query = |f: &Edn| {
        let q = if matches!(f, Edn::Map(_)) {
            f.get_kw("query")
        } else {
            Some(f)
        };
        matches!(q, Some(Edn::List(_) | Edn::Str(_)))
    };
    let forms = match read_all(&text) {
        Ok(f) if !f.first().is_some_and(dsl_query) => f,
        first_try => match read_all(&transformed) {
            Ok(f) if f.first().is_some_and(dsl_query) => f,
            _ => first_try.map_err(|e| QueryError::Syntax(e.to_string()))?,
        },
    };
    let Some(first) = forms.first() else {
        return Err(QueryError::Syntax("empty query".into()));
    };
    let mut warnings = Vec::new();
    let warn = |w: &mut Vec<Unsupported>, c: &str| {
        w.push(Unsupported {
            construct: c.to_owned(),
        });
    };

    let (query, title, collapsed, inputs): (&Edn, Option<String>, bool, Vec<Edn>) =
        if let Edn::Map(entries) = first {
            for (k, _) in entries {
                if let Edn::Kw(k) = k
                    && !COSMETIC_KEYS.contains(&k.as_str())
                {
                    warn(&mut warnings, &format!(":{k} (ignored)"));
                }
            }
            let Some(q) = first.get_kw("query") else {
                return Err(QueryError::Syntax("the query map has no :query".into()));
            };
            if first.get_kw("result-transform").is_some() {
                warn(&mut warnings, ":result-transform");
            }
            if first.get_kw("view").is_some() {
                warn(&mut warnings, ":view");
            }
            if matches!(first.get_kw("rules"), Some(Edn::Vector(v) | Edn::List(v)) if !v.is_empty())
            {
                warn(
                    &mut warnings,
                    ":rules (user-defined rules are not evaluated)",
                );
            }
            let title = match first.get_kw("title") {
                Some(Edn::Str(s)) => Some(s.clone()),
                _ => None,
            };
            let collapsed = matches!(first.get_kw("collapsed?"), Some(Edn::Bool(true)));
            let inputs = match first.get_kw("inputs") {
                Some(Edn::Vector(v) | Edn::List(v)) => v.clone(),
                Some(Edn::Nil) | None => Vec::new(),
                Some(other) => return Err(QueryError::Syntax(format!("bad :inputs {other}"))),
            };
            (q, title, collapsed, inputs)
        } else {
            (first, None, false, Vec::new())
        };

    match query {
        Edn::Vector(items) => {
            let mut out = run_datalog(conn, items, &inputs, ctx)?;
            out.title = title;
            out.collapsed = collapsed;
            warnings.append(&mut out.warnings);
            out.warnings = warnings;
            Ok(out)
        }
        Edn::List(_) | Edn::Str(_) => {
            let res = super::exec_simple(conn, &query.to_string(), ctx)?;
            let (columns, rows) = match res.kind {
                ResultKind::Blocks => (
                    vec!["?b".to_owned()],
                    res.blocks
                        .into_iter()
                        .map(|b| vec![Cell::Block(Box::new(b))])
                        .collect(),
                ),
                ResultKind::Pages => (
                    vec!["?p".to_owned()],
                    res.pages
                        .into_iter()
                        .map(|p| vec![Cell::Page(Box::new(p))])
                        .collect(),
                ),
            };
            Ok(AdvancedOutcome {
                title,
                collapsed,
                shape: Shape::Collection,
                columns,
                rows,
                warnings,
                sql: None,
            })
        }
        other => Err(QueryError::Syntax(format!(
            ":query must be a [:find ...] vector or a DSL form, got {other}"
        ))),
    }
}

/// Splits a `[:find ... :in ... :where ...]` vector into its sections.
fn sections(items: &[Edn]) -> Result<HashMap<String, Vec<Edn>>, QueryError> {
    let mut map: HashMap<String, Vec<Edn>> = HashMap::new();
    let mut cur: Option<String> = None;
    for it in items {
        if let Edn::Kw(k) = it {
            if !matches!(k.as_str(), "find" | "in" | "where" | "with") {
                return Err(QueryError::Unsupported(format!("query section :{k}")));
            }
            map.entry(k.clone()).or_default();
            cur = Some(k.clone());
        } else if let Some(c) = &cur {
            map.entry(c.clone()).or_default().push(it.clone());
        } else {
            return Err(QueryError::Syntax(
                "a Datalog query starts with :find".into(),
            ));
        }
    }
    Ok(map)
}

fn run_datalog(
    conn: &Connection,
    items: &[Edn],
    inputs: &[Edn],
    ctx: &QueryContext,
) -> Result<AdvancedOutcome, QueryError> {
    let sec = sections(items)?;
    let find = sec
        .get("find")
        .filter(|f| !f.is_empty())
        .ok_or_else(|| QueryError::Syntax("the query has no :find".into()))?;
    let where_ = sec.get("where").cloned().unwrap_or_default();
    let with = sec.get("with").cloned().unwrap_or_default();
    let in_vars: Vec<&Edn> = sec
        .get("in")
        .map(|v| {
            v.iter()
                .filter(|e| !matches!(e, Edn::Sym(s) if s == "$" || s == "%"))
                .collect()
        })
        .unwrap_or_default();
    if in_vars.len() != inputs.len() {
        return Err(QueryError::Syntax(format!(
            "the query takes {} input(s) but :inputs has {}",
            in_vars.len(),
            inputs.len()
        )));
    }

    let mut comp = Comp::new(ctx);
    comp.infer_all(&where_)?;
    let mut sc = comp.top_scope();
    for (v, input) in in_vars.iter().zip(inputs) {
        let Edn::Sym(name) = v else {
            return Err(QueryError::Unsupported(format!("input binding {v}")));
        };
        comp.bind_input(&mut sc, name, input)?;
    }
    comp.clauses(&mut sc, &where_)?;
    let (exprs, kinds, names, aggs, shape) = comp.find(&mut sc, find, &with)?;
    let limit_one = matches!(shape, Shape::Tuple | Shape::Scalar);
    let (sql, params) = assemble(&sc, &exprs, &aggs, limit_one).into_parts();
    let mut warnings: Vec<Unsupported> = comp
        .take_warnings()
        .into_iter()
        .map(|construct| Unsupported { construct })
        .collect();
    warnings.sort_by(|a, b| a.construct.cmp(&b.construct));
    warnings.dedup();

    register_regexp(conn)?;
    let mut st = conn.prepare(&sql)?;
    let raw: Vec<Vec<Value>> = st
        .query_map(params_from_iter(params.iter()), |r| {
            (0..kinds.len()).map(|i| r.get::<_, Value>(i)).collect()
        })?
        .collect::<Result<_, _>>()?;
    drop(st);

    let mut hyd = Hydrator::new(conn);
    let mut rows = Vec::with_capacity(raw.len());
    for r in raw {
        let mut row = Vec::with_capacity(r.len());
        for (v, k) in r.into_iter().zip(&kinds) {
            row.push(hyd.cell(v, *k)?);
        }
        rows.push(row);
    }
    Ok(AdvancedOutcome {
        title: None,
        collapsed: false,
        shape,
        columns: names,
        rows,
        warnings,
        sql: Some(sql),
    })
}

fn register_regexp(conn: &Connection) -> Result<(), QueryError> {
    conn.create_scalar_function(
        "bitacora_regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |c| {
            let re = c.get_or_create_aux(
                0,
                |v| -> Result<regex::Regex, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(regex::Regex::new(v.as_str()?)?)
                },
            )?;
            let t: Option<String> = c.get(1)?;
            Ok(t.is_some_and(|t| re.is_match(&t)))
        },
    )?;
    Ok(())
}

struct Hydrator<'c> {
    conn: &'c Connection,
    blocks: HashMap<i64, Option<BlockRow>>,
    pages: HashMap<i64, Option<PageRow>>,
}

impl<'c> Hydrator<'c> {
    fn new(conn: &'c Connection) -> Self {
        Self {
            conn,
            blocks: HashMap::new(),
            pages: HashMap::new(),
        }
    }

    fn cell(&mut self, v: Value, kind: ColKind) -> Result<Cell, QueryError> {
        match (kind, v) {
            (ColKind::Block, Value::Integer(id)) => {
                if !self.blocks.contains_key(&id) {
                    let mut b = self
                        .conn
                        .query_row(
                            &format!("SELECT {BLOCK_COLS} FROM blocks b WHERE b.id = ?1"),
                            [id],
                            block_from_row,
                        )
                        .optional()?;
                    if let Some(b) = b.as_mut() {
                        load_properties(self.conn, std::slice::from_mut(b))?;
                    }
                    self.blocks.insert(id, b);
                }
                Ok(self.blocks[&id]
                    .clone()
                    .map_or(Cell::Null, |b| Cell::Block(Box::new(b))))
            }
            (ColKind::Page, Value::Integer(id)) => {
                if !self.pages.contains_key(&id) {
                    let p = self
                        .conn
                        .query_row(
                            &format!("SELECT {PAGE_COLS} FROM pages p WHERE p.id = ?1"),
                            [id],
                            page_from_row,
                        )
                        .optional()?;
                    self.pages.insert(id, p);
                }
                Ok(self.pages[&id]
                    .clone()
                    .map_or(Cell::Null, |p| Cell::Page(Box::new(p))))
            }
            (_, Value::Null) => Ok(Cell::Null),
            (_, Value::Integer(n)) => Ok(Cell::Int(n)),
            (_, Value::Real(x)) => Ok(Cell::Real(x)),
            (_, Value::Text(s)) => Ok(Cell::Text(s)),
            (_, Value::Blob(_)) => Ok(Cell::Null),
        }
    }
}
