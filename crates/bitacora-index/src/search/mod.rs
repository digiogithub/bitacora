//! Full-text search (BIT-US-0009): query parser, ranked hybrid pipeline, fuzzy page titles,
//! snippets with highlight ranges and the `search.substring` switch.
//!
//! Pipeline (design `sqlite-index-schema.md` §6.2): exact page title or alias, title trigram,
//! nucleo fuzzy titles, bm25 block words and, when the word search comes up short, trigram or
//! `LIKE` substring blocks; the lists are fused with Reciprocal Rank Fusion (`k = 60`).

mod fuzzy;
pub(crate) use fuzzy::TitleCache;
mod query;
mod snippet;

use std::collections::HashMap;
use std::ops::Range;

use rusqlite::{Connection, params_from_iter};

use crate::Error;
use crate::schema::{FTS_TRIGGER_NAMES, FTS_TRIGGERS_NO_TRI_SQL, FTS_TRIGGERS_SQL};

pub use query::{Op, ParsedQuery, Term, quote};
pub use snippet::{DEFAULT_WINDOW, Snippet, build as build_snippet, find_matches};

/// Reciprocal Rank Fusion constant.
pub const RRF_K: f64 = 60.0;

/// Which part of the graph to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// Everything.
    #[default]
    All,
    /// Blocks of one page (`pages.id`); page titles are not searched.
    Page(i64),
    /// Journal pages and their blocks only.
    Journals,
    /// Non-journal pages and their blocks only.
    Pages,
}

/// Search settings.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum number of hits returned.
    pub limit: usize,
    /// Search scope.
    pub scope: Scope,
    /// Fold diacritics in the query; must match how the index was built (default on).
    pub remove_accents: bool,
    /// Snippet window in characters.
    pub snippet_window: usize,
    /// Today as `yyyyMMdd`; recent journal days get a small boost when set.
    pub today: Option<i64>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 50,
            scope: Scope::All,
            remove_accents: true,
            snippet_window: DEFAULT_WINDOW,
            today: None,
        }
    }
}

/// One ranked result.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchHit {
    /// A page matched by title or alias.
    Page {
        /// `pages.id`.
        page_id: i64,
        /// Display name.
        title: String,
        /// Journal page.
        is_journal: bool,
        /// The title with highlight ranges.
        snippet: Snippet,
        /// Fused score (higher is better).
        score: f64,
    },
    /// A block matched by content.
    Block {
        /// `blocks.id`.
        block_id: i64,
        /// Block UUID.
        uuid: String,
        /// `pages.id` of the containing page.
        page_id: i64,
        /// Display name of the containing page.
        page_title: String,
        /// The block lives in a journal.
        is_journal: bool,
        /// Window of the content with highlight ranges.
        snippet: Snippet,
        /// Fused score (higher is better).
        score: f64,
    },
}

impl SearchHit {
    /// The fused score.
    #[must_use]
    pub fn score(&self) -> f64 {
        match self {
            Self::Page { score, .. } | Self::Block { score, .. } => *score,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Page(i64),
    Block(i64),
}

/// A candidate from one ranked list.
#[derive(Debug, Clone, Copy)]
struct Cand {
    id: i64,
    journal_day: Option<i64>,
}

/// Runs the search pipeline over `conn` (a reader connection).
pub fn search(
    conn: &Connection,
    input: &str,
    opts: &SearchOptions,
) -> Result<Vec<SearchHit>, Error> {
    search_cached(conn, input, opts, None)
}

/// [`search`] with the page titles of the fuzzy pass taken from `titles` when given.
pub(crate) fn search_cached(
    conn: &Connection,
    input: &str,
    opts: &SearchOptions,
    titles: Option<&TitleCache>,
) -> Result<Vec<SearchHit>, Error> {
    let q = ParsedQuery::parse(input, opts.remove_accents);
    if q.is_empty() || opts.limit == 0 {
        return Ok(Vec::new());
    }
    let k = i64::try_from(opts.limit.saturating_mul(3).max(30)).unwrap_or(150);
    let plain = q.plain();
    let mut lists: Vec<(f64, Key, Vec<Cand>)> = Vec::new();

    if !matches!(opts.scope, Scope::Page(_)) {
        let exact = exact_pages(conn, input.trim(), opts.scope)?;
        lists.push((3.0, Key::Page(0), exact));
        lists.push((1.0, Key::Page(0), title_pages(conn, &q, opts.scope, k)?));
        lists.push((
            1.0,
            Key::Page(0),
            fuzzy::fuzzy_pages(conn, titles, &plain, opts.scope, 50)?,
        ));
    }
    let words = block_words(conn, &q, opts.scope, k)?;
    let short = words.len() < opts.limit;
    lists.push((1.0, Key::Block(0), words));
    if short {
        lists.push((
            1.0,
            Key::Block(0),
            block_substring(conn, &q, opts.scope, k)?,
        ));
    }

    // Reciprocal Rank Fusion, pages weighted above blocks.
    let mut scores: HashMap<Key, (f64, Option<i64>)> = HashMap::new();
    for (weight, kind, list) in &lists {
        for (rank, c) in list.iter().enumerate() {
            let key = match kind {
                Key::Page(_) => Key::Page(c.id),
                Key::Block(_) => Key::Block(c.id),
            };
            let page_boost = if matches!(key, Key::Page(_)) {
                1.25
            } else {
                1.0
            };
            let e = scores.entry(key).or_insert((0.0, c.journal_day));
            e.0 += weight * page_boost / (RRF_K + rank as f64 + 1.0);
        }
    }
    let mut ranked: Vec<(Key, f64)> = scores
        .into_iter()
        .map(|(key, (s, day))| (key, s * recency_boost(day, opts.today)))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.truncate(opts.limit);

    let terms: Vec<&str> = q.positive_terms().iter().map(|t| t.text.as_str()).collect();
    let mut out = Vec::with_capacity(ranked.len());
    for (key, score) in ranked {
        if let Some(hit) = hydrate(conn, key, score, &terms, opts)? {
            out.push(hit);
        }
    }
    Ok(out)
}

/// 1.0 up to 1.15 for journal days within the last year.
fn recency_boost(day: Option<i64>, today: Option<i64>) -> f64 {
    let (Some(d), Some(t)) = (day, today) else {
        return 1.0;
    };
    let ord = |x: i64| (x / 10_000) * 372 + (x / 100 % 100) * 31 + x % 100;
    let age = (ord(t) - ord(d)).clamp(0, 365);
    1.0 + 0.15 * (1.0 - age as f64 / 365.0)
}

fn scope_sql(scope: Scope, page_alias: &str, block_alias: Option<&str>) -> String {
    match scope {
        Scope::All => String::new(),
        Scope::Journals => format!(" AND {page_alias}.is_journal = 1"),
        Scope::Pages => format!(" AND {page_alias}.is_journal = 0"),
        Scope::Page(id) => match block_alias {
            Some(b) => format!(" AND {b}.page_id = {id}"),
            None => format!(" AND {page_alias}.id = {id}"),
        },
    }
}

fn cands(
    conn: &Connection,
    sql: &str,
    params: impl IntoIterator<Item = String>,
) -> Result<Vec<Cand>, Error> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(params_from_iter(params), |r| {
        Ok(Cand {
            id: r.get(0)?,
            journal_day: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn exact_pages(conn: &Connection, raw: &str, scope: Scope) -> Result<Vec<Cand>, Error> {
    let name = raw.to_lowercase();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    let s = scope_sql(scope, "p", None);
    let sql = format!(
        "SELECT p.id, p.journal_day FROM pages p WHERE p.name = ?1{s}
         UNION
         SELECT p.id, p.journal_day FROM pages ap
           JOIN page_aliases a ON a.alias_page_id = ap.id
           JOIN pages p ON p.id = a.page_id
         WHERE ap.name = ?1{s}"
    );
    cands(conn, &sql, [name])
}

fn title_pages(
    conn: &Connection,
    q: &ParsedQuery,
    scope: Scope,
    k: i64,
) -> Result<Vec<Cand>, Error> {
    let s = scope_sql(scope, "p", None);
    if let Some(m) = q.fts_trigram() {
        let sql = format!(
            "SELECT p.id, p.journal_day FROM pages_fts f JOIN pages p ON p.id = f.rowid
             WHERE pages_fts MATCH ?1{s} ORDER BY bm25(pages_fts), p.id LIMIT {k}"
        );
        return cands(conn, &sql, [m]);
    }
    let (pos, neg) = q.like_patterns();
    if pos.is_empty() {
        return Ok(Vec::new());
    }
    let (cond, params) = like_condition("p.search_title", pos, neg);
    let sql = format!(
        "SELECT p.id, p.journal_day FROM pages p WHERE {cond}{s} ORDER BY length(p.name), p.id LIMIT {k}"
    );
    cands(conn, &sql, params)
}

fn block_words(
    conn: &Connection,
    q: &ParsedQuery,
    scope: Scope,
    k: i64,
) -> Result<Vec<Cand>, Error> {
    let Some(m) = q.fts_words() else {
        return Ok(Vec::new());
    };
    let s = scope_sql(scope, "p", Some("b"));
    let sql = format!(
        "SELECT b.id, p.journal_day FROM blocks_fts
         JOIN blocks b ON b.id = blocks_fts.rowid JOIN pages p ON p.id = b.page_id
         WHERE blocks_fts MATCH ?1{s} ORDER BY bm25(blocks_fts), b.id LIMIT {k}"
    );
    cands(conn, &sql, [m])
}

/// Substring search: trigram when enabled and every term has three characters, else `LIKE`.
fn block_substring(
    conn: &Connection,
    q: &ParsedQuery,
    scope: Scope,
    k: i64,
) -> Result<Vec<Cand>, Error> {
    let s = scope_sql(scope, "p", Some("b"));
    if substring_enabled(conn)?
        && let Some(m) = q.fts_trigram()
    {
        let sql = format!(
            "SELECT b.id, p.journal_day FROM blocks_fts_tri
             JOIN blocks b ON b.id = blocks_fts_tri.rowid JOIN pages p ON p.id = b.page_id
             WHERE blocks_fts_tri MATCH ?1{s} ORDER BY bm25(blocks_fts_tri), b.id LIMIT {k}"
        );
        return cands(conn, &sql, [m]);
    }
    let (pos, neg) = q.like_patterns();
    if pos.is_empty() {
        return Ok(Vec::new());
    }
    let (cond, params) = like_condition("b.search_text", pos, neg);
    let sql = format!(
        "SELECT b.id, p.journal_day FROM blocks b JOIN pages p ON p.id = b.page_id
         WHERE {cond}{s} ORDER BY b.id LIMIT {k}"
    );
    cands(conn, &sql, params)
}

fn like_condition(col: &str, pos: Vec<String>, neg: Vec<String>) -> (String, Vec<String>) {
    let mut parts = Vec::new();
    let mut params = Vec::new();
    for p in pos {
        params.push(p);
        parts.push(format!("{col} LIKE ?{} ESCAPE '\\'", params.len()));
    }
    for n in neg {
        params.push(n);
        parts.push(format!("{col} NOT LIKE ?{} ESCAPE '\\'", params.len()));
    }
    (parts.join(" AND "), params)
}

fn hydrate(
    conn: &Connection,
    key: Key,
    score: f64,
    terms: &[&str],
    opts: &SearchOptions,
) -> Result<Option<SearchHit>, Error> {
    use rusqlite::OptionalExtension;
    Ok(match key {
        Key::Page(id) => conn
            .prepare_cached("SELECT original_name, is_journal FROM pages WHERE id = ?1")?
            .query_row([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))
            .optional()?
            .map(|(title, is_journal)| SearchHit::Page {
                page_id: id,
                snippet: snippet::build(&title, terms, opts.remove_accents, usize::MAX / 2),
                title,
                is_journal,
                score,
            }),
        Key::Block(id) => conn
            .prepare_cached(
                "SELECT b.uuid, b.page_id, p.original_name, p.is_journal, b.content
                 FROM blocks b JOIN pages p ON p.id = b.page_id WHERE b.id = ?1",
            )?
            .query_row([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .optional()?
            .map(
                |(uuid, page_id, page_title, is_journal, content)| SearchHit::Block {
                    block_id: id,
                    uuid,
                    page_id,
                    page_title,
                    is_journal,
                    snippet: snippet::build(
                        &content,
                        terms,
                        opts.remove_accents,
                        opts.snippet_window,
                    ),
                    score,
                },
            ),
    })
}

/// Highlight ranges of `terms` inside `text`, for callers that render their own rows.
#[must_use]
pub fn highlight(text: &str, query: &str, remove_accents: bool) -> Vec<Range<usize>> {
    let q = ParsedQuery::parse(query, remove_accents);
    let terms: Vec<&str> = q.positive_terms().iter().map(|t| t.text.as_str()).collect();
    find_matches(text, &terms, remove_accents)
}

/// Whether the trigram block index (`blocks_fts_tri`) exists.
pub fn substring_enabled(conn: &Connection) -> Result<bool, Error> {
    Ok(conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE name = 'blocks_fts_tri'",
        [],
        |r| r.get(0),
    )?)
}

/// Applies `search.substring`: `false` drops `blocks_fts_tri` (substring search falls back to
/// `LIKE`), `true` creates and fills it. Runs on the write connection in one transaction;
/// returns whether anything changed.
pub fn set_substring(conn: &Connection, enabled: bool) -> Result<bool, Error> {
    set_substring_in(conn, enabled, false)
}

/// [`set_substring`]; `in_bulk` leaves triggers and the rebuild to the bulk build's end.
pub(crate) fn set_substring_in(
    conn: &Connection,
    enabled: bool,
    in_bulk: bool,
) -> Result<bool, Error> {
    if substring_enabled(conn)? == enabled {
        return Ok(false);
    }
    let run = || -> Result<(), Error> {
        for name in FTS_TRIGGER_NAMES {
            conn.execute_batch(&format!("DROP TRIGGER IF EXISTS {name}"))?;
        }
        if enabled {
            conn.execute_batch(
                "CREATE VIRTUAL TABLE blocks_fts_tri USING fts5(
                   search_text, content = 'blocks', content_rowid = 'id', tokenize = 'trigram')",
            )?;
            if !in_bulk {
                conn.execute_batch("INSERT INTO blocks_fts_tri(blocks_fts_tri) VALUES('rebuild')")?;
            }
        } else {
            conn.execute_batch("DROP TABLE blocks_fts_tri")?;
        }
        if !in_bulk {
            conn.execute_batch(if enabled {
                FTS_TRIGGERS_SQL
            } else {
                FTS_TRIGGERS_NO_TRI_SQL
            })?;
        }
        Ok(())
    };
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match run() {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(true)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
