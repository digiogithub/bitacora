//! Machine-local semantic sync ledger and outbox (BIT-US-0143, ADR-030).
//!
//! # Why a separate SQLite file
//!
//! The ledger records what exists in the Pando server, which is state of *this machine's
//! relationship with that server*, not of the graph. It therefore must not live in the index
//! database (rule 5: the index is a cache that is deleted and rebuilt on any schema, parser or
//! corruption event, which would make us forget remote documents and leak orphans) and it must not
//! be in the graph folder (rule 1, and it would travel with git sync). It is a small SQLite file
//! next to the index (`<data_dir>/bitacora/graphs/<graph-id>/semantic.sqlite`): transactional
//! (an enqueue and its ledger update are atomic, a crash never loses an outbox row) and the same
//! engine the project already ships, unlike a JSON file that would need whole-file rewrites.
//!
//! # Tables
//!
//! * `semantic_state(doc_id, block_uuid, path, content_hash, synced_at)`: what was acknowledged by
//!   the server. It is the source of truth for purge and orphan cleanup: no remote listing is ever
//!   needed.
//! * `semantic_outbox(doc_id, op, ..., seq, attempts, next_at)`: at most one pending operation per
//!   document (the newest intent wins), retried with backoff.
//! * `semantic_meta`: the remote the state refers to; a different remote clears everything.

use std::collections::HashMap;
use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};

use super::SemanticError;

const USER_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS semantic_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS semantic_state (
  doc_id       TEXT PRIMARY KEY,
  block_uuid   TEXT NOT NULL,
  path         TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  synced_at    INTEGER NOT NULL
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS semantic_state_path ON semantic_state(path);
CREATE TABLE IF NOT EXISTS semantic_outbox (
  doc_id     TEXT PRIMARY KEY,
  op         TEXT NOT NULL CHECK (op IN ('upsert','delete')),
  block_uuid TEXT NOT NULL,
  path       TEXT NOT NULL,
  hash       TEXT NOT NULL,
  seq        INTEGER NOT NULL,
  attempts   INTEGER NOT NULL DEFAULT 0,
  next_at    INTEGER NOT NULL DEFAULT 0,
  last_error TEXT
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS semantic_outbox_due ON semantic_outbox(next_at, seq);
";

/// Pending operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Create or replace the document.
    Upsert,
    /// Remove the document.
    Delete,
}

impl Op {
    /// Text form (logs, SQL).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Delete => "delete",
        }
    }

    fn parse(s: &str) -> Self {
        if s == "delete" {
            Self::Delete
        } else {
            Self::Upsert
        }
    }
}

/// One row of the outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    /// Document id.
    pub doc_id: String,
    /// What to do.
    pub op: Op,
    /// Block UUID.
    pub block_uuid: String,
    /// Graph-relative file path at enqueue time.
    pub path: String,
    /// Content hash wanted remotely (the upsert hash; empty for deletes).
    pub hash: String,
    /// Monotonic sequence; a newer enqueue of the same document replaces the row.
    pub seq: i64,
    /// Failed attempts so far.
    pub attempts: u32,
    /// Earliest retry time (unix ms).
    pub next_at: i64,
}

/// One row of the state table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEntry {
    /// Document id.
    pub doc_id: String,
    /// Block UUID.
    pub block_uuid: String,
    /// Graph-relative file path of the last upsert.
    pub path: String,
    /// Hash of the last acknowledged upsert.
    pub content_hash: String,
}

/// What the ledger believes about a document: `Some(hash)` wanted (or present) remotely,
/// `None` a pending delete.
pub type Tracked = Option<String>;

/// The ledger (a SQLite connection behind a mutex; every call is short).
#[derive(Debug)]
pub struct Ledger {
    conn: Mutex<Connection>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl Ledger {
    /// Opens (creating) the ledger at `path` for the remote identified by `remote_key`.
    ///
    /// A ledger written for another remote, or by an incompatible version, is emptied: its
    /// documents live on a server we are no longer talking to, so everything is sent again.
    ///
    /// # Errors
    /// I/O or SQLite failures.
    pub fn open(path: &Path, remote_key: &str) -> Result<Self, SemanticError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| SemanticError::Io(e.to_string()))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        Self::init(conn, remote_key)
    }

    /// An in-memory ledger (tests).
    ///
    /// # Errors
    /// SQLite failures.
    pub fn open_in_memory(remote_key: &str) -> Result<Self, SemanticError> {
        Self::init(Connection::open_in_memory()?, remote_key)
    }

    fn init(conn: Connection, remote_key: &str) -> Result<Self, SemanticError> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 0 && version != USER_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS semantic_state; DROP TABLE IF EXISTS semantic_outbox; \
                 DROP TABLE IF EXISTS semantic_meta;",
            )?;
        }
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(&format!("PRAGMA user_version = {USER_VERSION};"))?;
        let ledger = Self {
            conn: Mutex::new(conn),
        };
        let known: Option<String> = ledger
            .conn
            .lock()
            .query_row(
                "SELECT value FROM semantic_meta WHERE key = 'remote'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if known.as_deref() != Some(remote_key) {
            ledger.reset()?;
            ledger.conn.lock().execute(
                "INSERT OR REPLACE INTO semantic_meta(key, value) VALUES ('remote', ?1)",
                [remote_key],
            )?;
        }
        Ok(ledger)
    }

    /// Forgets everything (state and outbox).
    ///
    /// # Errors
    /// SQLite failures.
    pub fn reset(&self) -> Result<(), SemanticError> {
        self.conn
            .lock()
            .execute_batch("DELETE FROM semantic_state; DELETE FROM semantic_outbox;")?;
        Ok(())
    }

    /// Wants `doc_id` present remotely with `hash`. Returns whether the outbox changed.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn enqueue_upsert(
        &self,
        doc_id: &str,
        block_uuid: &str,
        path: &str,
        hash: &str,
    ) -> Result<bool, SemanticError> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let state: Option<String> = tx
            .query_row(
                "SELECT content_hash FROM semantic_state WHERE doc_id = ?1",
                [doc_id],
                |r| r.get(0),
            )
            .optional()?;
        let pending: Option<(String, String)> = tx
            .query_row(
                "SELECT op, hash FROM semantic_outbox WHERE doc_id = ?1",
                [doc_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let changed = if state.as_deref() == Some(hash) {
            // Already there: whatever was pending (a stale upsert or a delete) is cancelled.
            tx.execute("DELETE FROM semantic_outbox WHERE doc_id = ?1", [doc_id])? > 0
        } else if pending
            .as_ref()
            .is_some_and(|(op, h)| op == "upsert" && h == hash)
        {
            false
        } else {
            tx.execute(
                "INSERT OR REPLACE INTO semantic_outbox \
                 (doc_id, op, block_uuid, path, hash, seq, attempts, next_at) \
                 VALUES (?1, 'upsert', ?2, ?3, ?4, \
                         (SELECT COALESCE(MAX(seq), 0) + 1 FROM semantic_outbox), 0, 0)",
                params![doc_id, block_uuid, path, hash],
            )?;
            true
        };
        tx.commit()?;
        Ok(changed)
    }

    /// Wants `doc_id` absent remotely. Returns whether the outbox changed.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn enqueue_delete(
        &self,
        doc_id: &str,
        block_uuid: &str,
        path: &str,
    ) -> Result<bool, SemanticError> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let in_state: bool = tx
            .query_row(
                "SELECT 1 FROM semantic_state WHERE doc_id = ?1",
                [doc_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let pending: Option<(String, u32)> = tx
            .query_row(
                "SELECT op, attempts FROM semantic_outbox WHERE doc_id = ?1",
                [doc_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        // An upsert that was already attempted may have reached the server before the crash or
        // the error, so it is turned into a delete instead of being forgotten.
        let maybe_remote = in_state
            || pending
                .as_ref()
                .is_some_and(|(op, a)| op == "upsert" && *a > 0);
        let changed = if !maybe_remote {
            tx.execute("DELETE FROM semantic_outbox WHERE doc_id = ?1", [doc_id])? > 0
        } else if pending.as_ref().is_some_and(|(op, _)| op == "delete") {
            false
        } else {
            tx.execute(
                "INSERT OR REPLACE INTO semantic_outbox \
                 (doc_id, op, block_uuid, path, hash, seq, attempts, next_at) \
                 VALUES (?1, 'delete', ?2, ?3, '', \
                         (SELECT COALESCE(MAX(seq), 0) + 1 FROM semantic_outbox), 0, 0)",
                params![doc_id, block_uuid, path],
            )?;
            true
        };
        tx.commit()?;
        Ok(changed)
    }

    /// What the ledger believes about the documents of the file at `path`: acknowledged state,
    /// overlaid with pending outbox operations on that path.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn tracked_for_path(
        &self,
        path: &str,
    ) -> Result<HashMap<String, (String, Tracked)>, SemanticError> {
        let conn = self.conn.lock();
        let mut out: HashMap<String, (String, Tracked)> = HashMap::new();
        {
            let mut st = conn.prepare_cached(
                "SELECT doc_id, block_uuid, content_hash FROM semantic_state WHERE path = ?1",
            )?;
            for row in st.query_map([path], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (id, uuid, hash) = row?;
                out.insert(id, (uuid, Some(hash)));
            }
        }
        let mut st = conn.prepare_cached(
            "SELECT doc_id, block_uuid, op, hash FROM semantic_outbox WHERE path = ?1",
        )?;
        for row in st.query_map([path], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })? {
            let (id, uuid, op, hash) = row?;
            let t = if Op::parse(&op) == Op::Delete {
                None
            } else {
                Some(hash)
            };
            out.insert(id, (uuid, t));
        }
        Ok(out)
    }

    /// Distinct file paths that have acknowledged documents or pending operations.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn known_paths(&self) -> Result<Vec<String>, SemanticError> {
        let conn = self.conn.lock();
        let mut st = conn.prepare_cached(
            "SELECT path FROM semantic_state UNION SELECT path FROM semantic_outbox ORDER BY 1",
        )?;
        Ok(st
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?)
    }

    /// Outbox rows ready at `now` (unix ms), oldest first.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn due(&self, now: i64, limit: usize) -> Result<Vec<OutboxEntry>, SemanticError> {
        let conn = self.conn.lock();
        let mut st = conn.prepare_cached(
            "SELECT doc_id, op, block_uuid, path, hash, seq, attempts, next_at \
             FROM semantic_outbox WHERE next_at <= ?1 ORDER BY seq LIMIT ?2",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        Ok(st
            .query_map(params![now, limit], |r| {
                Ok(OutboxEntry {
                    doc_id: r.get(0)?,
                    op: Op::parse(&r.get::<_, String>(1)?),
                    block_uuid: r.get(2)?,
                    path: r.get(3)?,
                    hash: r.get(4)?,
                    seq: r.get(5)?,
                    attempts: r.get(6)?,
                    next_at: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    /// Earliest `next_at` among pending rows.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn next_due_at(&self) -> Result<Option<i64>, SemanticError> {
        Ok(self
            .conn
            .lock()
            .query_row("SELECT MIN(next_at) FROM semantic_outbox", [], |r| r.get(0))?)
    }

    /// Records that `entry` reached the server. For an upsert, `sent` is the document that was
    /// actually sent (it may be fresher than the enqueued hash). The outbox row is removed only
    /// when it was not replaced in the meantime.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn complete(
        &self,
        entry: &OutboxEntry,
        sent: Option<&StateEntry>,
    ) -> Result<(), SemanticError> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        match (entry.op, sent) {
            (Op::Upsert, Some(s)) => {
                tx.execute(
                    "INSERT OR REPLACE INTO semantic_state \
                     (doc_id, block_uuid, path, content_hash, synced_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![s.doc_id, s.block_uuid, s.path, s.content_hash, now_ms()],
                )?;
            }
            _ => {
                tx.execute(
                    "DELETE FROM semantic_state WHERE doc_id = ?1",
                    [&entry.doc_id],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM semantic_outbox WHERE doc_id = ?1 AND seq = ?2",
            params![entry.doc_id, entry.seq],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Records a failed attempt; the row is retried at `next_at`.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn fail(
        &self,
        entry: &OutboxEntry,
        next_at: i64,
        error: &str,
    ) -> Result<(), SemanticError> {
        self.conn.lock().execute(
            "UPDATE semantic_outbox SET attempts = attempts + 1, next_at = ?3, last_error = ?4 \
             WHERE doc_id = ?1 AND seq = ?2",
            params![entry.doc_id, entry.seq, next_at, error],
        )?;
        Ok(())
    }

    /// Postpones every pending row until `until` (unix ms) without counting an attempt: the server
    /// is unreachable, so retrying row by row would only hammer it.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn defer_all(&self, until: i64) -> Result<(), SemanticError> {
        self.conn.lock().execute(
            "UPDATE semantic_outbox SET next_at = MAX(next_at, ?1)",
            [until],
        )?;
        Ok(())
    }

    /// Makes every pending row due now (after a reconnect or a manual retry).
    ///
    /// # Errors
    /// SQLite failures.
    pub fn retry_now(&self) -> Result<(), SemanticError> {
        self.conn
            .lock()
            .execute("UPDATE semantic_outbox SET next_at = 0", [])?;
        Ok(())
    }

    /// Schedules the deletion of every document the server holds for this graph and cancels
    /// pending upserts (consent revoked, "remove from Pando"). Returns the number of deletes.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn purge_all(&self) -> Result<usize, SemanticError> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        // Upserts that never reached the server vanish; attempted ones become deletes below.
        tx.execute(
            "DELETE FROM semantic_outbox WHERE op = 'upsert' AND attempts = 0 \
             AND doc_id NOT IN (SELECT doc_id FROM semantic_state)",
            [],
        )?;
        let n = tx.execute(
            "INSERT OR REPLACE INTO semantic_outbox \
             (doc_id, op, block_uuid, path, hash, seq, attempts, next_at) \
             SELECT doc_id, 'delete', block_uuid, path, '', \
                    (SELECT COALESCE(MAX(seq), 0) FROM semantic_outbox) + 1, 0, 0 \
             FROM semantic_state",
            [],
        )?;
        // Attempted upserts without state: the server may hold them.
        tx.execute(
            "UPDATE semantic_outbox SET op = 'delete', hash = '', attempts = 0, next_at = 0 \
             WHERE op = 'upsert'",
            [],
        )?;
        tx.commit()?;
        Ok(n)
    }

    fn count(&self, table: &str) -> Result<u64, SemanticError> {
        let n: i64 =
            self.conn
                .lock()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Number of documents acknowledged by the server.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn synced_count(&self) -> Result<u64, SemanticError> {
        self.count("semantic_state")
    }

    /// Number of pending outbox rows.
    ///
    /// # Errors
    /// SQLite failures.
    pub fn pending_count(&self) -> Result<u64, SemanticError> {
        self.count("semantic_outbox")
    }

    /// The error of the most recent failed send still waiting in the outbox (cleared when the row
    /// is acknowledged).
    ///
    /// # Errors
    /// SQLite failures.
    pub fn last_error(&self) -> Result<Option<String>, SemanticError> {
        let conn = self.conn.lock();
        let mut st = conn.prepare_cached(
            "SELECT last_error FROM semantic_outbox WHERE last_error IS NOT NULL \
             ORDER BY attempts DESC, seq DESC LIMIT 1",
        )?;
        let mut rows = st.query([])?;
        match rows.next()? {
            Some(row) => Ok(row.get(0)?),
            None => Ok(None),
        }
    }

    /// Acknowledged documents (tests and diagnostics).
    ///
    /// # Errors
    /// SQLite failures.
    pub fn state(&self) -> Result<Vec<StateEntry>, SemanticError> {
        let conn = self.conn.lock();
        let mut st = conn.prepare_cached(
            "SELECT doc_id, block_uuid, path, content_hash FROM semantic_state ORDER BY doc_id",
        )?;
        Ok(st
            .query_map([], |r| {
                Ok(StateEntry {
                    doc_id: r.get(0)?,
                    block_uuid: r.get(1)?,
                    path: r.get(2)?,
                    content_hash: r.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod last_error_tests {
    use super::*;

    #[test]
    fn last_error_reports_the_failed_send_until_it_is_acknowledged() {
        let ledger = Ledger::open_in_memory("remote").expect("ledger");
        assert_eq!(ledger.last_error().expect("none"), None);
        ledger
            .enqueue_upsert("doc", "uuid", "pages/A.md", "h1")
            .expect("enqueue");
        let entry = ledger.due(i64::MAX, 10).expect("due").remove(0);
        ledger.fail(&entry, 0, "boom").expect("fail");
        assert_eq!(ledger.last_error().expect("error").as_deref(), Some("boom"));
        ledger.complete(&entry, None).expect("complete");
        assert_eq!(ledger.last_error().expect("none"), None);
    }
}
