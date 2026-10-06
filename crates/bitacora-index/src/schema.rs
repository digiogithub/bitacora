//! Schema v1: embedded DDL, version constants and connection pragmas.

use rusqlite::Connection;

/// Schema version, stored in `PRAGMA user_version` and `meta.schema_version`.
/// A mismatch deletes and recreates the database (no migrations; ADR-004).
pub const SCHEMA_VERSION: i64 = 1;

/// Default parser version recorded in `meta.parser_version`. Callers (the reindex
/// pipeline) pass their own through `OpenOptions`.
pub const DEFAULT_PARSER_VERSION: i64 = 1;

/// Default FTS normalizer version recorded in `meta.normalizer_version`.
pub const DEFAULT_NORMALIZER_VERSION: i64 = 1;

/// The schema v1 DDL (tables, FTS5 tables, triggers, views), without `user_version`.
pub const SCHEMA_V1_SQL: &str = concat!(
    include_str!("schema_v1.sql"),
    "\n",
    include_str!("fts_triggers.sql")
);

/// The FTS sync triggers, recreated after the cold-build fast path.
pub(crate) const FTS_TRIGGERS_SQL: &str = include_str!("fts_triggers.sql");

/// The FTS sync triggers when the trigram block index is absent (`search.substring = false`).
pub(crate) const FTS_TRIGGERS_NO_TRI_SQL: &str = include_str!("fts_triggers_no_tri.sql");

/// Names of the FTS sync triggers (dropped for the cold-build fast path).
pub(crate) const FTS_TRIGGER_NAMES: [&str; 6] = [
    "blocks_ai",
    "blocks_ad",
    "blocks_au",
    "pages_ai",
    "pages_ad",
    "pages_au",
];

/// Pragmas applied to every connection (read and write).
const COMMON_PRAGMAS: &str = "
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 268435456;
PRAGMA cache_size = -65536;
PRAGMA busy_timeout = 5000;
";

/// Apply the pragmas of the write connection (WAL, relaxed sync, foreign keys).
pub(crate) fn apply_write_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    // `journal_mode` returns a row, so it goes through `query_row`.
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    conn.execute_batch(COMMON_PRAGMAS)
}

/// Apply the pragmas of a read-only pool connection.
pub(crate) fn apply_read_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA query_only = ON;")?;
    conn.execute_batch(COMMON_PRAGMAS)
}

/// Create schema v1 in a fresh database inside one transaction and stamp `user_version`.
pub(crate) fn create_schema(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA_V1_SQL)?;
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
    tx.commit()
}

/// Check that the SQLite build has the FTS5 tokenizers the schema needs.
pub(crate) fn check_features(conn: &Connection) -> Result<(), crate::Error> {
    let probe = |tokenizer: &str, name: &'static str| -> Result<(), crate::Error> {
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE temp.__fts_probe USING fts5(x, tokenize = '{tokenizer}');
             DROP TABLE temp.__fts_probe;"
        ))
        .map_err(|_| crate::Error::MissingFeature(name))
    };
    probe("unicode61 remove_diacritics 2", "fts5 unicode61")?;
    probe("trigram", "fts5 trigram")
}
