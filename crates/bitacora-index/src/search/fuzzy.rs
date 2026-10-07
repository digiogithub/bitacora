//! Fuzzy page titles with `nucleo-matcher` (BIT-T-0070): a subsequence match over every page
//! title and alias (`pages.search_title`, already folded), ranked by score.
//!
//! The title list is cached per reader pool and refreshed when the writer bumps the pool's
//! generation, so a query no longer reads every title from SQLite (BIT-T-0336).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use parking_lot::Mutex;
use rusqlite::Connection;

use super::{Cand, Scope};
use crate::Error;

/// One page for the fuzzy pass.
#[derive(Debug)]
pub(crate) struct TitleRow {
    id: i64,
    journal_day: Option<i64>,
    is_journal: bool,
    title: String,
}

/// Every page title, shared by the readers of one pool and valid for one writer generation.
#[derive(Debug)]
pub(crate) struct TitleCache {
    generation: Arc<AtomicU64>,
    slot: Mutex<Option<(u64, Arc<Vec<TitleRow>>)>>,
}

impl TitleCache {
    pub(crate) fn new(generation: Arc<AtomicU64>) -> Self {
        Self {
            generation,
            slot: Mutex::new(None),
        }
    }

    /// The titles as of the current generation (loaded on a miss).
    fn rows(&self, conn: &Connection) -> Result<Arc<Vec<TitleRow>>, Error> {
        // Read the generation first: a write that lands during the load bumps it again, so the
        // next call reloads rather than serving the half-seen state.
        let generation = self.generation.load(Ordering::Acquire);
        if let Some((g, rows)) = &*self.slot.lock()
            && *g == generation
        {
            return Ok(Arc::clone(rows));
        }
        let rows = Arc::new(load_titles(conn)?);
        *self.slot.lock() = Some((generation, Arc::clone(&rows)));
        Ok(rows)
    }
}

fn load_titles(conn: &Connection) -> Result<Vec<TitleRow>, Error> {
    let mut stmt = conn
        .prepare_cached("SELECT p.id, p.journal_day, p.is_journal, p.search_title FROM pages p")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(TitleRow {
            id: r.get(0)?,
            journal_day: r.get(1)?,
            is_journal: r.get::<_, i64>(2)? != 0,
            title: r.get(3)?,
        });
    }
    Ok(out)
}

fn in_scope(scope: Scope, row: &TitleRow) -> bool {
    match scope {
        Scope::All => true,
        Scope::Journals => row.is_journal,
        Scope::Pages => !row.is_journal,
        Scope::Page(id) => row.id == id,
    }
}

/// Top `limit` pages whose title fuzzy-matches `plain` (whitespace-separated atoms must all
/// match). Needs at least two characters: a single letter would match nearly everything.
pub(super) fn fuzzy_pages(
    conn: &Connection,
    cache: Option<&TitleCache>,
    plain: &str,
    scope: Scope,
    limit: usize,
) -> Result<Vec<Cand>, Error> {
    if plain.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let pattern = Pattern::new(
        plain,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let titles = match cache {
        Some(c) => c.rows(conn)?,
        None => Arc::new(load_titles(conn)?),
    };
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize, &TitleRow)> = Vec::new();
    for row in titles.iter().filter(|r| in_scope(scope, r)) {
        if let Some(score) = pattern.score(Utf32Str::new(&row.title, &mut buf), &mut matcher) {
            scored.push((score, row.title.len(), row));
        }
    }
    // Best score first; shorter titles, then lower ids, break ties deterministically.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.id.cmp(&b.2.id)));
    scored.truncate(limit);
    Ok(scored
        .into_iter()
        .map(|(_, _, r)| Cand {
            id: r.id,
            journal_day: r.journal_day,
        })
        .collect())
}
