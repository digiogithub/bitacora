//! Diagnostics read API (BIT-SP-0003.R11): rows per file and per kind for the UI, MCP and CLI.

use rusqlite::{params_from_iter, types::Value};
use serde::Serialize;

use super::IndexReader;
use crate::Error;
use crate::parsed::DiagnosticKind;

/// One stored diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticRow {
    /// Row id.
    pub id: i64,
    /// Relative path of the file, `None` for graph-wide diagnostics.
    pub file: Option<String>,
    /// Kind (an unknown stored kind is reported as `parse_error`).
    pub kind: DiagnosticKind,
    /// 0 info, 1 warning, 2 error.
    pub severity: i64,
    /// 1-based line.
    pub line: Option<i64>,
    /// Human-readable message.
    pub message: String,
    /// Extra JSON payload, as stored.
    pub data: Option<String>,
}

/// Filter of [`IndexReader::diagnostics`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticFilter {
    /// Only this file (relative path).
    pub file: Option<String>,
    /// Only this kind.
    pub kind: Option<DiagnosticKind>,
    /// Only severities at or above this.
    pub min_severity: Option<i64>,
}

/// Number of diagnostics of one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DiagnosticCount {
    /// Kind.
    pub kind: DiagnosticKind,
    /// Rows of that kind.
    pub count: usize,
}

impl IndexReader {
    /// Diagnostics matching the filter, ordered by file then line.
    pub fn diagnostics(&self, filter: &DiagnosticFilter) -> Result<Vec<DiagnosticRow>, Error> {
        let mut sql = String::from(
            "SELECT d.id, f.path, d.kind, d.severity, d.line, d.message, d.data \
             FROM diagnostics d LEFT JOIN files f ON f.id = d.file_id WHERE 1 = 1",
        );
        let mut args: Vec<Value> = Vec::new();
        if let Some(file) = &filter.file {
            sql.push_str(" AND f.path = ?");
            args.push(Value::Text(file.clone()));
        }
        if let Some(kind) = filter.kind {
            sql.push_str(" AND d.kind = ?");
            args.push(Value::Text(kind.as_str().to_owned()));
        }
        if let Some(min) = filter.min_severity {
            sql.push_str(" AND d.severity >= ?");
            args.push(Value::Integer(min));
        }
        sql.push_str(" ORDER BY f.path, d.line, d.id");
        let conn = self.conn()?;
        let mut st = conn.prepare(&sql)?;
        let rows = st
            .query_map(params_from_iter(args), |r| {
                let kind: String = r.get(2)?;
                Ok(DiagnosticRow {
                    id: r.get(0)?,
                    file: r.get(1)?,
                    kind: DiagnosticKind::from_name(&kind).unwrap_or(DiagnosticKind::ParseError),
                    severity: r.get(3)?,
                    line: r.get(4)?,
                    message: r.get(5)?,
                    data: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Row counts per kind (kinds without rows are omitted), sorted by kind name.
    pub fn diagnostic_counts(&self) -> Result<Vec<DiagnosticCount>, Error> {
        let conn = self.conn()?;
        let mut st =
            conn.prepare("SELECT kind, count(*) FROM diagnostics GROUP BY kind ORDER BY kind")?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(k, n)| {
                Some(DiagnosticCount {
                    kind: DiagnosticKind::from_name(&k)?,
                    count: usize::try_from(n).unwrap_or(0),
                })
            })
            .collect())
    }
}
