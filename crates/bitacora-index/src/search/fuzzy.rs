//! Fuzzy page titles with `nucleo-matcher` (BIT-T-0070): a subsequence match over every page
//! title and alias (`pages.search_title`, already folded), ranked by score.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rusqlite::Connection;

use super::{Cand, Scope, scope_sql};
use crate::Error;

/// Top `limit` pages whose title fuzzy-matches `plain` (whitespace-separated atoms must all
/// match). Needs at least two characters: a single letter would match nearly everything.
pub(super) fn fuzzy_pages(
    conn: &Connection,
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
    let sql = format!(
        "SELECT p.id, p.journal_day, p.search_title FROM pages p WHERE 1 = 1{}",
        scope_sql(scope, "p", None)
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query([])?;
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize, Cand)> = Vec::new();
    while let Some(r) = rows.next()? {
        let title: String = r.get(2)?;
        if let Some(score) = pattern.score(Utf32Str::new(&title, &mut buf), &mut matcher) {
            scored.push((
                score,
                title.len(),
                Cand {
                    id: r.get(0)?,
                    journal_day: r.get(1)?,
                },
            ));
        }
    }
    // Best score first; shorter titles, then lower ids, break ties deterministically.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.id.cmp(&b.2.id)));
    scored.truncate(limit);
    Ok(scored.into_iter().map(|(_, _, c)| c).collect())
}
