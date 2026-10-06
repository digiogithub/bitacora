//! Unlinked references (BIT-SP-0003.R17): an FTS phrase prefilter over the page name and its
//! aliases, then Logseq's mention regex in Rust. `blocks` is never scanned without the prefilter.

use regex::Regex;
use rusqlite::params;

use super::refs::{RefGroup, group_hits};
use super::{BlockRow, IndexReader, block_from_row};
use crate::Error;
use crate::normalize::normalize_query;

/// FTS prefilter: blocks whose normalised text contains one of the quoted phrases.
pub const UNLINKED_FTS_SQL: &str = "SELECT {COLS} FROM blocks_fts f \
    CROSS JOIN blocks b ON b.id = f.rowid WHERE blocks_fts MATCH ?1 AND b.page_id <> ?2";

/// Remove `:LOGBOOK:` ... `:END:` drawers.
fn strip_logbook(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find(":LOGBOOK:") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find(":END:") {
            Some(end) => rest = &after[end + ":END:".len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Logseq's mention pattern for one page name.
fn mention_regex(name: &str) -> Option<Regex> {
    let escaped = regex::escape(name);
    Regex::new(&format!(
        r"(?i)(^|[^\[#0-9a-zA-Z]|((^|[^\[])\[)){escaped}($|[^0-9a-zA-Z])"
    ))
    .ok()
}

impl IndexReader {
    /// Blocks that mention the page (or one of its aliases) as plain text, outside the page
    /// itself and not already referencing it. Grouped like linked references.
    pub fn unlinked_references(&self, page_id: i64) -> Result<Vec<RefGroup>, Error> {
        let closure = self.alias_closure(page_id)?;
        let conn = self.conn()?;
        let mut names: Vec<String> = Vec::new();
        {
            let mut st = conn.prepare_cached("SELECT original_name FROM pages WHERE id = ?1")?;
            for id in &closure {
                if let Ok(n) = st.query_row([id], |r| r.get::<_, String>(0))
                    && !n.trim().is_empty()
                    && !names.contains(&n)
                {
                    names.push(n);
                }
            }
        }
        let regexes: Vec<Regex> = names.iter().filter_map(|n| mention_regex(n)).collect();
        let phrases: Vec<String> = names
            .iter()
            .map(|n| normalize_query(n, true))
            .filter(|n| n.chars().any(char::is_alphanumeric))
            .map(|n| format!("\"{}\"", n.replace('"', "\"\"")))
            .collect();
        if phrases.is_empty() || regexes.is_empty() {
            return Ok(Vec::new());
        }
        let mut st = conn.prepare_cached(&super::outline::with_cols(UNLINKED_FTS_SQL))?;
        let mut candidates: Vec<BlockRow> = st
            .query_map(params![phrases.join(" OR "), page_id], block_from_row)?
            .collect::<Result<_, _>>()?;
        drop(st);
        // No ORDER BY in the prefilter: it would let the planner walk `blocks` instead of the FTS hits.
        candidates.sort_by_key(|b| (b.page_id, b.file_id, b.ord));

        let mut linked = conn
            .prepare_cached("SELECT 1 FROM block_page_refs WHERE block_id = ?1 AND page_id = ?2")?;
        let mut hits = Vec::new();
        for b in candidates {
            let text = strip_logbook(&b.content);
            if !regexes.iter().any(|re| re.is_match(&text)) {
                continue;
            }
            let mut already = false;
            for id in &closure {
                if linked.exists(params![b.id, id])? {
                    already = true;
                    break;
                }
            }
            if !already {
                hits.push(b);
            }
        }
        drop(linked);
        group_hits(&conn, hits, false)
    }
}
