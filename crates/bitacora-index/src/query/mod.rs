//! Query engine (BIT-US-0101, BIT-US-0103): the simple `{{query ...}}` DSL and a Datalog subset
//! for advanced queries, both compiled to parameterised SQL over the index
//! (`docs/design/sqlite-index-schema.md` §7 and §8).
//!
//! * [`IndexReader::query_simple`] parses, compiles and runs a simple query.
//! * [`IndexReader::query_advanced`] runs a `#+BEGIN_QUERY` block (see [`advanced`]).
//!
//! Compilation is pure: [`compile_simple`] needs no connection, so it can be tested and its SQL
//! inspected. Every user value is a bound parameter, never spliced into the SQL text.

pub mod advanced;
mod compile;
mod datalog;
mod dates;
pub mod dsl;
pub mod edn;

use bitacora_config::EffectiveConfig;
use bitacora_core::date::Date;
use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::Error;
use crate::read::{BlockRow, IndexReader, PageRow, block_from_row, load_properties, page_from_row};

pub use advanced::{AdvancedOutcome, Cell, Unsupported};
pub use dsl::{PropValue, Query, SimpleQuery, SortBy, parse as parse_simple};

/// Errors of the query engine.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// The query text is malformed.
    #[error("syntax error: {0}")]
    Syntax(String),
    /// The query uses a construct that is not supported (`unsupported: <construct>`).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The database failed.
    #[error(transparent)]
    Index(#[from] Error),
}

impl From<rusqlite::Error> for QueryError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Index(Error::Sqlite(e))
    }
}

/// Everything a query needs to know about "now" and where it runs.
#[derive(Debug, Clone)]
pub struct QueryContext {
    /// Today's date (local), for `today`, `-7d`, ...
    pub today: Date,
    /// Current time, unix ms (`now`).
    pub now_ms: i64,
    /// Start of today, unix ms (the base of timestamp offsets). Defaults to UTC midnight.
    pub today_start_ms: i64,
    /// Journal title formats used to read `[[Oct 5th, 2026]]` arguments.
    pub journal_formatters: Vec<String>,
    /// Page the query block is on (advanced inputs `:current-page` / `:query-page`), page key.
    pub current_page: Option<String>,
    /// UUID of the block being edited or run from (advanced input `:current-block`).
    pub current_block: Option<String>,
    /// UUID of the block that holds the query; excluded from simple results.
    pub query_block: Option<String>,
    /// Fold diacritics in `"text"` filters (matches the index normaliser).
    pub remove_accents: bool,
    /// Use the trigram FTS index for `"text"` (the reader turns it off when it is absent).
    pub trigram: bool,
    /// Maximum rows returned by a simple query.
    pub limit: Option<usize>,
}

impl QueryContext {
    /// A context for `today` and `now_ms` with default settings.
    #[must_use]
    pub fn new(today: Date, now_ms: i64) -> Self {
        Self {
            today,
            now_ms,
            today_start_ms: dates::days_from_civil(today) * 86_400_000,
            journal_formatters: bitacora_core::journal::journal_title_formatters(
                &EffectiveConfig::default(),
            ),
            current_page: None,
            current_block: None,
            query_block: None,
            remove_accents: true,
            trigram: true,
            limit: None,
        }
    }
}

/// What a simple query returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    /// Block rows.
    Blocks,
    /// Page rows.
    Pages,
}

/// A compiled simple query.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// SQL text with `?` placeholders.
    pub sql: String,
    /// Positional parameters.
    pub params: Vec<Value>,
    /// Result type decided by Logseq's rule.
    pub kind: ResultKind,
}

/// Result of a simple query. Exactly one of `blocks` / `pages` is filled, per `kind`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    /// Blocks or pages.
    pub kind: ResultKind,
    /// Matching blocks (block mode).
    pub blocks: Vec<BlockRow>,
    /// Matching pages (page mode).
    pub pages: Vec<PageRow>,
}

impl PartialEq for Compiled {
    fn eq(&self, o: &Self) -> bool {
        self.sql == o.sql && self.kind == o.kind
    }
}

/// Parses and compiles a simple query without touching a database.
pub fn compile_simple(src: &str, ctx: &QueryContext) -> Result<Compiled, QueryError> {
    compile::compile(&dsl::parse(src)?, ctx)
}

/// Parses, compiles and runs a simple query on `conn`.
pub(crate) fn exec_simple(
    conn: &rusqlite::Connection,
    src: &str,
    ctx: &QueryContext,
) -> Result<QueryResult, QueryError> {
    let mut ctx = ctx.clone();
    ctx.trigram = ctx.trigram && crate::search::substring_enabled(conn)?;
    let c = compile_simple(src, &ctx)?;
    let mut st = conn.prepare(&c.sql)?;
    let mut out = QueryResult {
        kind: c.kind,
        blocks: Vec::new(),
        pages: Vec::new(),
    };
    match c.kind {
        ResultKind::Blocks => {
            out.blocks = st
                .query_map(params_from_iter(c.params.iter()), block_from_row)?
                .collect::<Result<_, _>>()?;
            load_properties(conn, &mut out.blocks)?;
        }
        ResultKind::Pages => {
            out.pages = st
                .query_map(params_from_iter(c.params.iter()), page_from_row)?
                .collect::<Result<_, _>>()?;
        }
    }
    Ok(out)
}

impl IndexReader {
    /// Runs a simple `{{query ...}}` DSL query.
    pub fn query_simple(&self, src: &str, ctx: &QueryContext) -> Result<QueryResult, QueryError> {
        let conn = self.conn()?;
        exec_simple(&conn, src, ctx)
    }
    /// Runs an advanced query given as the text of a `#+BEGIN_QUERY ... #+END_QUERY` block (or
    /// just its EDN map). See [`advanced`] for the supported subset.
    pub fn query_advanced(
        &self,
        src: &str,
        ctx: &QueryContext,
    ) -> Result<AdvancedOutcome, QueryError> {
        let conn = self.conn()?;
        advanced::run(&conn, src, ctx)
    }
}
