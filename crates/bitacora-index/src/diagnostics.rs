//! Index integrity checks and the `doctor` report (BIT-SP-0003.R1, R11).
//!
//! [`inspect_index`] never opens the database through [`crate::Index::open`] (which deletes a
//! corrupt file); it looks at the file as it is and reports problems instead of repairing them.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::pool::ReaderPool;
use crate::read::{DiagnosticCount, DiagnosticFilter, DiagnosticRow, IndexReader};

/// Result of one integrity check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckResult {
    /// Check name (`quick_check`, `foreign_key_check`, `fts:blocks_fts`, ...).
    pub name: String,
    /// The check passed.
    pub ok: bool,
    /// What was found (`ok` or the first problems).
    pub detail: String,
}

impl CheckResult {
    fn pass(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ok: true,
            detail: "ok".to_owned(),
        }
    }

    fn from_error(name: &str, e: &rusqlite::Error) -> Self {
        Self {
            name: name.to_owned(),
            ok: false,
            detail: e.to_string(),
        }
    }
}

/// Table row counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct IndexStats {
    /// Rows of `files`.
    pub files: i64,
    /// Rows of `pages`.
    pub pages: i64,
    /// Rows of `blocks`.
    pub blocks: i64,
    /// Rows of `diagnostics`.
    pub diagnostics: i64,
}

impl IndexStats {
    /// Count rows of an open index database.
    pub fn read(conn: &Connection) -> Result<Self, rusqlite::Error> {
        let n = |t: &str| conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0));
        Ok(Self {
            files: n("files")?,
            pages: n("pages")?,
            blocks: n("blocks")?,
            diagnostics: n("diagnostics")?,
        })
    }
}

/// Everything `bitacora-cli doctor` prints about the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    /// Database path.
    pub path: PathBuf,
    /// The file exists.
    pub exists: bool,
    /// Size in bytes (0 when missing).
    pub size_bytes: u64,
    /// `meta` key/values (schema, parser, normalizer, config hash, ...), sorted by key.
    pub meta: Vec<(String, String)>,
    /// Version of the SQLite library in this process.
    pub sqlite_version: String,
    /// Row counts, when the tables could be read.
    pub stats: Option<IndexStats>,
    /// Integrity checks that ran.
    pub checks: Vec<CheckResult>,
    /// Stored diagnostics.
    pub diagnostics: Vec<DiagnosticRow>,
    /// Diagnostics per kind.
    pub diagnostic_counts: Vec<DiagnosticCount>,
}

impl DoctorReport {
    /// Every integrity check passed (and the index exists).
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.exists && self.checks.iter().all(|c| c.ok)
    }
}

/// Inspect the index file at `db_path` without modifying it: run `quick_check`,
/// `foreign_key_check` and the FTS5 `integrity-check` of the three FTS tables, and collect
/// metadata and diagnostics. Failures are reported in [`DoctorReport::checks`], never returned
/// as errors, so a corrupt file still yields a report.
#[must_use]
pub fn inspect_index(db_path: &Path) -> DoctorReport {
    let mut report = DoctorReport {
        path: db_path.to_owned(),
        exists: db_path.is_file(),
        size_bytes: std::fs::metadata(db_path).map_or(0, |m| m.len()),
        meta: Vec::new(),
        sqlite_version: rusqlite::version().to_owned(),
        stats: None,
        checks: Vec::new(),
        diagnostics: Vec::new(),
        diagnostic_counts: Vec::new(),
    };
    if !report.exists {
        return report;
    }
    let conn = match Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            report.checks.push(CheckResult::from_error("open", &e));
            return report;
        }
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(5000));

    let mut structural_ok = true;
    match conn.prepare("PRAGMA quick_check").and_then(|mut st| {
        st.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()
    }) {
        Ok(rows) if rows == ["ok"] => report.checks.push(CheckResult::pass("quick_check")),
        Ok(rows) => {
            structural_ok = false;
            report.checks.push(CheckResult {
                name: "quick_check".to_owned(),
                ok: false,
                detail: rows.iter().take(5).cloned().collect::<Vec<_>>().join("; "),
            });
        }
        Err(e) => {
            structural_ok = false;
            report
                .checks
                .push(CheckResult::from_error("quick_check", &e));
        }
    }
    if !structural_ok {
        return report;
    }

    match conn.prepare("PRAGMA foreign_key_check").and_then(|mut st| {
        st.query_map([], |r| {
            Ok(format!(
                "{}#{}",
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
    }) {
        Ok(rows) if rows.is_empty() => report.checks.push(CheckResult::pass("foreign_key_check")),
        Ok(rows) => report.checks.push(CheckResult {
            name: "foreign_key_check".to_owned(),
            ok: false,
            detail: format!(
                "{} violations, first: {}",
                rows.len(),
                rows.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            ),
        }),
        Err(e) => report
            .checks
            .push(CheckResult::from_error("foreign_key_check", &e)),
    }

    for table in ["blocks_fts", "blocks_fts_tri", "pages_fts"] {
        let name = format!("fts:{table}");
        match conn.execute(
            &format!("INSERT INTO {table}({table}) VALUES('integrity-check')"),
            [],
        ) {
            Ok(_) => report.checks.push(CheckResult::pass(&name)),
            Err(e) => report.checks.push(CheckResult::from_error(&name, &e)),
        }
    }

    if let Ok(mut st) = conn.prepare("SELECT key, value FROM meta ORDER BY key")
        && let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    {
        report.meta = rows.filter_map(Result::ok).collect();
    }
    if report.meta.iter().any(|(k, _)| k == "bulk_in_progress") {
        report.checks.push(CheckResult {
            name: "bulk_build".to_owned(),
            ok: false,
            detail: "a cold build was interrupted".to_owned(),
        });
    }
    match IndexStats::read(&conn) {
        Ok(s) => report.stats = Some(s),
        Err(e) => report.checks.push(CheckResult::from_error("tables", &e)),
    }
    drop(conn);

    let reader = IndexReader::new(ReaderPool::new(db_path, 1));
    match reader.diagnostics(&DiagnosticFilter::default()) {
        Ok(d) => report.diagnostics = d,
        Err(e) => report.checks.push(CheckResult {
            name: "diagnostics".to_owned(),
            ok: false,
            detail: e.to_string(),
        }),
    }
    if let Ok(c) = reader.diagnostic_counts() {
        report.diagnostic_counts = c;
    }
    report
}
