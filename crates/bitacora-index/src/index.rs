//! Open and validate lifecycle (design §1 principle 2, §4.1 step 1).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

use crate::error::Error;
use crate::location::IndexLocation;
use crate::pool::{PooledReader, ReaderPool};
use crate::schema::{
    DEFAULT_NORMALIZER_VERSION, DEFAULT_PARSER_VERSION, SCHEMA_VERSION, apply_write_pragmas,
    check_features, create_schema,
};

/// Inputs of the validation step: what the running application expects to find in `meta`.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// Graph root, recorded in `meta.graph_root` (informational).
    pub graph_root: PathBuf,
    /// Version of the parser that produces rows; a change flags a full reparse.
    pub parser_version: i64,
    /// Version of the FTS text normalizer; a change flags an FTS-only rebuild.
    pub normalizer_version: i64,
    /// Hash of the config keys that affect indexing (design §4.1 step 1); a change flags a full reparse.
    pub config_hash: String,
    /// Maximum number of read-only connections in the pool.
    pub reader_pool_size: usize,
}

impl OpenOptions {
    /// Options with default parser/normalizer versions and a pool of 4 readers.
    pub fn new(graph_root: impl Into<PathBuf>, config_hash: impl Into<String>) -> Self {
        Self {
            graph_root: graph_root.into(),
            parser_version: DEFAULT_PARSER_VERSION,
            normalizer_version: DEFAULT_NORMALIZER_VERSION,
            config_hash: config_hash.into(),
            reader_pool_size: 4,
        }
    }
}

/// Why the database file was deleted and recreated on open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecreateReason {
    /// `user_version` differs from [`SCHEMA_VERSION`] (no migrations: delete and rebuild).
    SchemaVersionMismatch,
    /// SQLite reported corruption, the file is not a database, `quick_check` failed or `meta` is unreadable.
    Corrupt,
}

/// How much of the index must be recomputed from the graph after open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RebuildKind {
    /// The index is up to date; only the stat/hash reconcile is needed.
    None,
    /// Only `search_text` and the FTS tables must be rebuilt (`normalizer_version` changed).
    FtsOnly,
    /// Every file must be reparsed (new/recreated DB, `parser_version` or `config_hash` changed).
    FullReparse,
}

/// Values read from `meta` before any update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVersions {
    /// Stored parser version.
    pub parser_version: i64,
    /// Stored normalizer version.
    pub normalizer_version: i64,
    /// Stored config hash.
    pub config_hash: String,
}

/// Result of [`Index::open`]: what happened and what the caller must do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOutcome {
    /// A new database was created (first run, or after deletion).
    pub created: bool,
    /// Set when an existing database was deleted before creating a new one.
    pub recreated: Option<RecreateReason>,
    /// Required recomputation. Persisted only by [`WriteConnection::record_versions`], so
    /// the flag survives a crash before the rebuild finishes.
    pub rebuild: RebuildKind,
    /// Versions found in an existing database, `None` when freshly created.
    pub stored: Option<StoredVersions>,
}

/// An open index: validated database, reader pool and the single write connection.
#[derive(Debug)]
pub struct Index {
    location: IndexLocation,
    options: OpenOptions,
    outcome: OpenOutcome,
    readers: ReaderPool,
    writer_slot: Arc<Mutex<Option<Connection>>>,
}

impl Index {
    /// Open (creating, validating or recreating as needed) the index for a graph.
    ///
    /// Version mismatch or corruption deletes the database file (and its WAL/SHM) and creates a
    /// fresh one; `parser_version`/`config_hash`/`normalizer_version` mismatches only set
    /// [`OpenOutcome::rebuild`]. Nothing is ever written inside the graph folder.
    pub fn open(location: IndexLocation, options: OpenOptions) -> Result<Self, Error> {
        let dir = location.dir();
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let path = location.db_path();

        let (conn, outcome) = match validate_existing(&path, &options)? {
            Validation::Ok { conn, outcome } => (*conn, outcome),
            Validation::Missing => (create_fresh(&path, &options)?, fresh_outcome(None)),
            Validation::Recreate(reason) => {
                remove_db_files(&path)?;
                (create_fresh(&path, &options)?, fresh_outcome(Some(reason)))
            }
        };

        let readers = ReaderPool::new(&path, options.reader_pool_size);
        Ok(Self {
            location,
            options,
            outcome,
            readers,
            writer_slot: Arc::new(Mutex::new(Some(conn))),
        })
    }

    /// What happened during open and which rebuild (if any) is required.
    pub fn outcome(&self) -> &OpenOutcome {
        &self.outcome
    }

    /// Where the database lives.
    pub fn location(&self) -> &IndexLocation {
        &self.location
    }

    /// Options the index was opened with.
    pub fn options(&self) -> &OpenOptions {
        &self.options
    }

    /// Check out a read-only connection from the pool.
    pub fn reader(&self) -> Result<PooledReader, Error> {
        self.readers.get()
    }

    /// The reader pool, to hand to search/UI components.
    pub fn readers(&self) -> &ReaderPool {
        &self.readers
    }

    /// Take ownership of the one write connection. It returns to the index when dropped;
    /// while it is out, further calls fail with [`Error::WriterTaken`].
    pub fn take_writer(&self) -> Result<WriteConnection, Error> {
        let conn = self.writer_slot.lock().take().ok_or(Error::WriterTaken)?;
        Ok(WriteConnection {
            conn: Some(conn),
            slot: Arc::clone(&self.writer_slot),
            options: self.options.clone(),
        })
    }
}

/// The single write connection (WAL writer). Owned by exactly one component at a time.
#[derive(Debug)]
pub struct WriteConnection {
    conn: Option<Connection>,
    slot: Arc<Mutex<Option<Connection>>>,
    options: OpenOptions,
}

impl WriteConnection {
    /// Shared access to the connection.
    pub fn conn(&self) -> &Connection {
        match &self.conn {
            Some(c) => c,
            None => unreachable!("write connection used after drop"),
        }
    }

    /// Mutable access to the connection (needed to open transactions).
    pub fn conn_mut(&mut self) -> &mut Connection {
        match &mut self.conn {
            Some(c) => c,
            None => unreachable!("write connection used after drop"),
        }
    }

    /// Persist the expected parser/normalizer/config values into `meta`. Call after the
    /// rebuild requested by [`OpenOutcome::rebuild`] completed successfully.
    pub fn record_versions(&mut self) -> Result<(), Error> {
        let opts = self.options.clone();
        let tx = self.conn_mut().transaction()?;
        write_meta(&tx, &opts)?;
        tx.commit()?;
        Ok(())
    }
}

impl Drop for WriteConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            *self.slot.lock() = Some(conn);
        }
    }
}

enum Validation {
    Missing,
    Recreate(RecreateReason),
    Ok {
        conn: Box<Connection>,
        outcome: OpenOutcome,
    },
}

fn fresh_outcome(recreated: Option<RecreateReason>) -> OpenOutcome {
    OpenOutcome {
        created: true,
        recreated,
        rebuild: RebuildKind::FullReparse,
        stored: None,
    }
}

fn is_corrupt(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if matches!(f.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

/// Run a fallible rusqlite step; corruption maps to `Ok(Err(Recreate))`, other errors propagate.
fn corrupt_or<T>(r: rusqlite::Result<T>) -> Result<Result<T, Validation>, Error> {
    match r {
        Ok(v) => Ok(Ok(v)),
        Err(e) if is_corrupt(&e) => Ok(Err(Validation::Recreate(RecreateReason::Corrupt))),
        Err(e) => Err(e.into()),
    }
}

fn validate_existing(path: &Path, options: &OpenOptions) -> Result<Validation, Error> {
    if !path.exists() {
        return Ok(Validation::Missing);
    }
    let conn = match Connection::open(path) {
        Ok(c) => c,
        Err(e) if is_corrupt(&e) => return Ok(Validation::Recreate(RecreateReason::Corrupt)),
        Err(e) => return Err(e.into()),
    };
    // Busy handler before anything else so a concurrent process does not fail validation.
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;

    macro_rules! step {
        ($e:expr) => {
            match corrupt_or($e)? {
                Ok(v) => v,
                Err(v) => return Ok(v),
            }
        };
    }

    let version: i64 = step!(conn.query_row("PRAGMA user_version", [], |r| r.get(0)));
    if version != SCHEMA_VERSION {
        let tables: i64 = step!(conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0)
        ));
        // An empty file (e.g. creation interrupted before the schema transaction committed)
        // is not a mismatch: it is just a database to initialize.
        if version == 0 && tables == 0 {
            return Ok(Validation::Missing);
        }
        return Ok(Validation::Recreate(RecreateReason::SchemaVersionMismatch));
    }

    let check: String = step!(conn.query_row("PRAGMA quick_check", [], |r| r.get(0)));
    if check != "ok" {
        return Ok(Validation::Recreate(RecreateReason::Corrupt));
    }

    let has_meta: bool = step!(conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
        [],
        |r| r.get(0)
    ));
    if !has_meta {
        return Ok(Validation::Recreate(RecreateReason::Corrupt));
    }

    let get = |key: &str| -> rusqlite::Result<Option<String>> {
        conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
    };
    let stored_schema = step!(get("schema_version"));
    let parser = step!(get("parser_version"));
    let normalizer = step!(get("normalizer_version"));
    let config = step!(get("config_hash"));
    let (Some(stored_schema), Some(parser), Some(normalizer), Some(config)) =
        (stored_schema, parser, normalizer, config)
    else {
        return Ok(Validation::Recreate(RecreateReason::Corrupt));
    };
    let (Ok(parser), Ok(normalizer)) = (parser.parse::<i64>(), normalizer.parse::<i64>()) else {
        return Ok(Validation::Recreate(RecreateReason::Corrupt));
    };
    if stored_schema.parse::<i64>().ok() != Some(SCHEMA_VERSION) {
        return Ok(Validation::Recreate(RecreateReason::SchemaVersionMismatch));
    }

    let rebuild = if parser != options.parser_version || config != options.config_hash {
        RebuildKind::FullReparse
    } else if normalizer != options.normalizer_version {
        RebuildKind::FtsOnly
    } else {
        RebuildKind::None
    };

    apply_write_pragmas(&conn)?;
    Ok(Validation::Ok {
        conn: Box::new(conn),
        outcome: OpenOutcome {
            created: false,
            recreated: None,
            rebuild,
            stored: Some(StoredVersions {
                parser_version: parser,
                normalizer_version: normalizer,
                config_hash: config,
            }),
        },
    })
}

fn create_fresh(path: &Path, options: &OpenOptions) -> Result<Connection, Error> {
    let mut conn = Connection::open(path)?;
    apply_write_pragmas(&conn)?;
    check_features(&conn)?;
    create_schema(&mut conn)?;
    let tx = conn.transaction()?;
    write_meta(&tx, options)?;
    tx.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES ('created_at', CAST(unixepoch('subsec') * 1000 AS INTEGER))",
        [],
    )?;
    tx.commit()?;
    Ok(conn)
}

fn write_meta(conn: &Connection, options: &OpenOptions) -> rusqlite::Result<()> {
    let sqlite_version: String = conn.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let graph_root = options.graph_root.to_string_lossy().into_owned();
    let mut stmt =
        conn.prepare_cached("INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)")?;
    for (k, v) in [
        ("schema_version", SCHEMA_VERSION.to_string()),
        ("parser_version", options.parser_version.to_string()),
        ("normalizer_version", options.normalizer_version.to_string()),
        ("config_hash", options.config_hash.clone()),
        ("graph_root", graph_root),
        ("sqlite_version", sqlite_version),
    ] {
        stmt.execute(params![k, v])?;
    }
    Ok(())
}

/// Delete the database and its `-wal` / `-shm` side files. Missing files are fine.
fn remove_db_files(path: &Path) -> Result<(), Error> {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let p = PathBuf::from(name);
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::io(p, e)),
        }
    }
    Ok(())
}
