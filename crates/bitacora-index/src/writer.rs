//! The single writer thread (ADR-004, design §4.4): owns the write connection, takes jobs from a
//! bounded channel, runs each in one transaction and emits [`IndexEvent`]s after commit.
//!
//! Bulk mode (design §4.1 step 6, the cold-build fast path) drops the FTS triggers and foreign-key
//! checks, groups many files per transaction and rebuilds the FTS tables when it ends.

use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::Error;
use crate::index::WriteConnection;
use crate::replace::{
    DeleteOutcome, FileInput, ReplaceOutcome, WriteOptions, delete_file, rename_file_row,
    replace_file, seed_builtin_pages, touch_file,
};
use crate::schema::{FTS_TRIGGER_NAMES, FTS_TRIGGERS_NO_TRI_SQL, FTS_TRIGGERS_SQL};

/// Files per transaction in bulk mode (Logseq batches 100; design §4.1 step 6 says about 200).
pub const BULK_BATCH_FILES: usize = 200;

/// Notification emitted after a write committed, so the UI can refresh pages, live queries and
/// backlink panels (design §4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexEvent {
    /// A file was (re)indexed.
    FileReplaced {
        /// `files.id`.
        file_id: i64,
        /// Graph-relative path.
        path: String,
        /// Pages whose content or references changed.
        page_ids_touched: Vec<i64>,
        /// Block UUIDs that appeared.
        block_uuids_added: Vec<String>,
        /// Block UUIDs that disappeared.
        block_uuids_removed: Vec<String>,
    },
    /// A file left the index.
    FileDeleted {
        /// Graph-relative path.
        path: String,
        /// Pages affected (the page became a placeholder or was collected).
        page_ids_touched: Vec<i64>,
        /// Block UUIDs that disappeared.
        block_uuids_removed: Vec<String>,
    },
    /// A file moved; a [`IndexEvent::FileReplaced`] for the new path follows.
    FileRenamed {
        /// Old path.
        from: String,
        /// New path.
        to: String,
    },
    /// A bulk (cold build) run finished: everything may have changed.
    BulkFinished,
}

type Reply<T> = Sender<Result<T, Error>>;

/// The result of a job that is still running on the writer thread.
#[derive(Debug)]
pub struct Pending<T>(Receiver<Result<T, Error>>);

impl<T> Pending<T> {
    /// Block until the writer finished the job.
    pub fn wait(self) -> Result<T, Error> {
        self.0.recv().unwrap_or(Err(Error::WriterStopped))
    }
}

enum Job {
    Replace(Box<FileInput>, Reply<ReplaceOutcome>),
    Delete(String, Reply<Option<DeleteOutcome>>),
    Rename {
        from: String,
        to: String,
        input: Box<FileInput>,
        reply: Reply<Option<ReplaceOutcome>>,
    },
    Touch {
        path: String,
        size: u64,
        mtime_ns: i64,
        reply: Reply<()>,
    },
    BeginBulk {
        truncate: bool,
        reply: Reply<()>,
    },
    EndBulk(Reply<()>),
    RecordVersions(Reply<()>),
    SetMeta(String, String, Reply<()>),
    SetConfigHash(String, Reply<()>),
    Flush(Reply<()>),
    Renormalize(Reply<()>),
    SetSubstring(bool, Reply<bool>),
    Stop,
}

type Subscribers = Arc<Mutex<Vec<Sender<IndexEvent>>>>;

/// Handle to the writer thread. Dropping it stops the thread and returns the write connection to
/// the [`crate::Index`].
#[derive(Debug)]
pub struct IndexWriter {
    tx: SyncSender<Job>,
    subscribers: Subscribers,
    join: Option<JoinHandle<()>>,
}

impl IndexWriter {
    /// Start the writer thread on `conn`, seeding the built-in pages.
    pub fn spawn(conn: WriteConnection, opts: WriteOptions) -> Result<Self, Error> {
        seed_builtin_pages(conn.conn(), &opts)?;
        let (tx, rx) = sync_channel::<Job>(256);
        let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));
        let subs = Arc::clone(&subscribers);
        let join = std::thread::Builder::new()
            .name("bitacora-index-writer".into())
            .spawn(move || {
                let mut state = Writer {
                    conn,
                    opts,
                    subs,
                    bulk: None,
                };
                state.run(&rx);
            })
            .map_err(|e| Error::io("<writer thread>", e))?;
        Ok(Self {
            tx,
            subscribers,
            join: Some(join),
        })
    }

    /// Receive the events emitted from now on.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<IndexEvent> {
        let (tx, rx) = channel();
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push(tx);
        }
        rx
    }

    fn submit<T>(&self, make: impl FnOnce(Reply<T>) -> Job) -> Pending<T> {
        let (tx, rx) = channel();
        // A stopped writer drops the reply sender, which `wait` reports as `WriterStopped`.
        let _ = self.tx.send(make(tx));
        Pending(rx)
    }

    /// Queue a file replace without waiting.
    pub fn submit_replace(&self, input: FileInput) -> Pending<ReplaceOutcome> {
        self.submit(|r| Job::Replace(Box::new(input), r))
    }

    /// Replace a file and wait for the commit.
    pub fn replace_file(&self, input: FileInput) -> Result<ReplaceOutcome, Error> {
        self.submit_replace(input).wait()
    }

    /// Queue removal of a file (no-op result `None` when unknown).
    pub fn submit_delete(&self, path: &str) -> Pending<Option<DeleteOutcome>> {
        self.submit(|r| Job::Delete(path.to_owned(), r))
    }

    /// Remove a file from the index and wait.
    pub fn delete_file(&self, path: &str) -> Result<Option<DeleteOutcome>, Error> {
        self.submit_delete(path).wait()
    }

    /// Move `from` to `to` keeping the file row, then replace it with `input` (the parse of the
    /// new path) so block UUIDs carry over. `None` when `from` is not indexed (nothing is written).
    pub fn rename_file(
        &self,
        from: &str,
        to: &str,
        input: FileInput,
    ) -> Result<Option<ReplaceOutcome>, Error> {
        self.submit(|reply| Job::Rename {
            from: from.to_owned(),
            to: to.to_owned(),
            input: Box::new(input),
            reply,
        })
        .wait()
    }

    /// Update size/mtime only (content hash unchanged).
    pub fn touch_file(&self, path: &str, size: u64, mtime_ns: i64) -> Pending<()> {
        self.submit(|reply| Job::Touch {
            path: path.to_owned(),
            size,
            mtime_ns,
            reply,
        })
    }

    /// Enter bulk mode (cold build). `truncate` empties every table first (full reparse).
    pub fn begin_bulk(&self, truncate: bool) -> Result<(), Error> {
        self.submit(|reply| Job::BeginBulk { truncate, reply })
            .wait()
    }

    /// Leave bulk mode: commit, rebuild the FTS tables, recreate triggers, check, `ANALYZE`.
    pub fn end_bulk(&self) -> Result<(), Error> {
        self.submit(Job::EndBulk).wait()
    }

    /// Persist the expected parser/normalizer/config versions (after a finished rebuild).
    pub fn record_versions(&self) -> Result<(), Error> {
        self.submit(Job::RecordVersions).wait()
    }

    /// Change the config hash [`IndexWriter::record_versions`] persists (config hot reload).
    pub fn set_config_hash(&self, hash: &str) -> Result<(), Error> {
        self.submit(|r| Job::SetConfigHash(hash.to_owned(), r))
            .wait()
    }

    /// Set a `meta` key.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), Error> {
        self.submit(|r| Job::SetMeta(key.to_owned(), value.to_owned(), r))
            .wait()
    }

    /// Recompute `search_text` / `search_title` with the current normalizer and rebuild the FTS
    /// tables (`RebuildKind::FtsOnly`).
    pub fn renormalize_search(&self) -> Result<(), Error> {
        self.submit(Job::Renormalize).wait()
    }

    /// Apply `search.substring`: drop (`false`) or create and fill (`true`) the trigram block
    /// index `blocks_fts_tri`. Returns whether the index changed.
    pub fn set_substring(&self, enabled: bool) -> Result<bool, Error> {
        self.submit(|r| Job::SetSubstring(enabled, r)).wait()
    }

    /// Wait until every job queued before this call has been processed.
    pub fn flush(&self) -> Result<(), Error> {
        self.submit(Job::Flush).wait()
    }

    /// Stop the thread (after pending jobs) and give the write connection back to the index.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Job::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for IndexWriter {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Bulk {
    files_in_tx: usize,
    in_tx: bool,
}

struct Writer {
    conn: WriteConnection,
    opts: WriteOptions,
    subs: Subscribers,
    bulk: Option<Bulk>,
}

impl Writer {
    fn run(&mut self, rx: &Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            match job {
                Job::Stop => break,
                Job::Replace(input, reply) => {
                    let r = self.in_tx(|c, o| replace_file(c, o, &input));
                    if let Ok(out) = &r {
                        self.emit_replaced(&input.path, out);
                    }
                    let _ = reply.send(r);
                }
                Job::Delete(path, reply) => {
                    let r = self.in_tx(|c, o| delete_file(c, o, &path));
                    if let Ok(Some(out)) = &r {
                        self.emit(IndexEvent::FileDeleted {
                            path,
                            page_ids_touched: out.page_ids_touched.clone(),
                            block_uuids_removed: out.block_uuids_removed.clone(),
                        });
                    }
                    let _ = reply.send(r);
                }
                Job::Rename {
                    from,
                    to,
                    input,
                    reply,
                } => {
                    let r = self.in_tx(|c, o| {
                        if rename_file_row(c, o, &from, &to)? {
                            replace_file(c, o, &input).map(Some)
                        } else {
                            Ok(None)
                        }
                    });
                    if let Ok(Some(out)) = &r {
                        self.emit(IndexEvent::FileRenamed { from, to });
                        self.emit_replaced(&input.path, out);
                    }
                    let _ = reply.send(r);
                }
                Job::Touch {
                    path,
                    size,
                    mtime_ns,
                    reply,
                } => {
                    let r = self.in_tx(|c, _| touch_file(c, &path, size, mtime_ns));
                    let _ = reply.send(r);
                }
                Job::BeginBulk { truncate, reply } => {
                    let _ = reply.send(self.begin_bulk(truncate));
                }
                Job::EndBulk(reply) => {
                    let _ = reply.send(self.end_bulk());
                }
                Job::RecordVersions(reply) => {
                    let _ = reply.send(self.conn.record_versions());
                }
                Job::SetConfigHash(hash, reply) => {
                    self.conn.set_config_hash(hash);
                    let _ = reply.send(Ok(()));
                }
                Job::SetMeta(k, v, reply) => {
                    let r = self
                        .conn
                        .conn()
                        .execute(
                            "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
                            [k, v],
                        )
                        .map(|_| ())
                        .map_err(Error::from);
                    let _ = reply.send(r);
                }
                Job::Renormalize(reply) => {
                    let _ = reply.send(self.renormalize());
                }
                Job::SetSubstring(enabled, reply) => {
                    let r = self.commit_bulk_batch().and_then(|()| {
                        crate::search::set_substring_in(
                            self.conn.conn(),
                            enabled,
                            self.bulk.is_some(),
                        )
                    });
                    let _ = reply.send(r);
                }
                Job::Flush(reply) => {
                    let _ = reply.send(self.commit_bulk_batch());
                }
            }
        }
        // Leaving with a bulk transaction open would lose it: commit what was written.
        if self.bulk.is_some() {
            let _ = self.end_bulk();
        }
    }

    fn emit(&self, event: IndexEvent) {
        if self.bulk.is_some() {
            return;
        }
        if let Ok(mut subs) = self.subs.lock() {
            subs.retain(|s| s.send(event.clone()).is_ok());
        }
    }

    fn emit_replaced(&self, path: &bitacora_core::graph_path::GraphPath, out: &ReplaceOutcome) {
        self.emit(IndexEvent::FileReplaced {
            file_id: out.file_id,
            path: path.as_str().to_owned(),
            page_ids_touched: out.page_ids_touched.clone(),
            block_uuids_added: out.block_uuids_added.clone(),
            block_uuids_removed: out.block_uuids_removed.clone(),
        });
    }

    /// Run `f` atomically: its own `BEGIN IMMEDIATE`/`COMMIT` normally, a savepoint inside the
    /// open batch transaction in bulk mode.
    fn in_tx<T>(
        &mut self,
        f: impl FnOnce(&Connection, &WriteOptions) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let opts = self.opts;
        if let Some(bulk) = &mut self.bulk {
            let conn = self.conn.conn();
            if !bulk.in_tx {
                conn.execute_batch("BEGIN IMMEDIATE")?;
                bulk.in_tx = true;
            }
            conn.execute_batch("SAVEPOINT job")?;
            return match f(conn, &opts) {
                Ok(v) => {
                    conn.execute_batch("RELEASE job")?;
                    bulk.files_in_tx += 1;
                    if bulk.files_in_tx >= BULK_BATCH_FILES {
                        conn.execute_batch("COMMIT")?;
                        bulk.in_tx = false;
                        bulk.files_in_tx = 0;
                    }
                    Ok(v)
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK TO job; RELEASE job");
                    Err(e)
                }
            };
        }
        let conn = self.conn.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        match f(conn, &opts) {
            Ok(v) => {
                conn.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    fn commit_bulk_batch(&mut self) -> Result<(), Error> {
        if let Some(bulk) = &mut self.bulk
            && bulk.in_tx
        {
            self.conn.conn().execute_batch("COMMIT")?;
            bulk.in_tx = false;
            bulk.files_in_tx = 0;
        }
        Ok(())
    }

    fn renormalize(&mut self) -> Result<(), Error> {
        self.begin_bulk(false)?;
        let r = self.in_tx(|conn, opts| {
            let rows: Vec<(i64, String)> = conn
                .prepare("SELECT id, content FROM blocks")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            let mut set =
                conn.prepare_cached("UPDATE blocks SET search_text = ?2 WHERE id = ?1")?;
            for (id, content) in rows {
                let (text, _) = crate::normalize::search_text(&content, opts.normalize);
                set.execute(rusqlite::params![id, text])?;
            }
            let pages: Vec<i64> = conn
                .prepare("SELECT id FROM pages")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            crate::replace::recompute_search_titles(conn, opts, &pages)
        });
        self.end_bulk()?;
        r
    }

    fn begin_bulk(&mut self, truncate: bool) -> Result<(), Error> {
        if self.bulk.is_some() {
            return Ok(());
        }
        let conn = self.conn.conn();
        // Marks the index as half-built until `finish_bulk`; a crash in between forces a rebuild.
        conn.execute_batch(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('bulk_in_progress', '1')",
        )?;
        conn.execute_batch("PRAGMA foreign_keys = OFF")?;
        for name in FTS_TRIGGER_NAMES {
            conn.execute_batch(&format!("DROP TRIGGER IF EXISTS {name}"))?;
        }
        if truncate {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            conn.execute_batch(
                "DELETE FROM diagnostics; DELETE FROM block_property_values; DELETE FROM block_properties;
                 DELETE FROM block_block_refs; DELETE FROM block_page_refs; DELETE FROM page_aliases;
                 DELETE FROM page_tags; DELETE FROM blocks; DELETE FROM pages; DELETE FROM files;",
            )?;
            seed_builtin_pages(conn, &self.opts)?;
            conn.execute_batch("COMMIT")?;
        }
        self.bulk = Some(Bulk {
            files_in_tx: 0,
            in_tx: false,
        });
        Ok(())
    }

    fn end_bulk(&mut self) -> Result<(), Error> {
        if self.bulk.is_none() {
            return Ok(());
        }
        let result = self.finish_bulk();
        self.bulk = None;
        // Always restore the invariants, even when the FTS rebuild failed.
        let conn = self.conn.conn();
        let _ = conn.execute_batch("PRAGMA foreign_keys = ON");
        result?;
        self.emit(IndexEvent::BulkFinished);
        Ok(())
    }

    fn finish_bulk(&mut self) -> Result<(), Error> {
        self.commit_bulk_batch()?;
        let conn = self.conn.conn();
        rebuild_fts(conn)?;
        conn.execute_batch(if crate::search::substring_enabled(conn)? {
            FTS_TRIGGERS_SQL
        } else {
            FTS_TRIGGERS_NO_TRI_SQL
        })?;
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let violations = stmt.query_map([], |_| Ok(()))?.count();
        drop(stmt);
        if violations > 0 {
            return Err(Error::ForeignKeyViolations(violations));
        }
        conn.execute_batch("DELETE FROM meta WHERE key = 'bulk_in_progress'")?;
        conn.execute_batch("ANALYZE")?;
        Ok(())
    }
}

/// `INSERT INTO <fts>(<fts>) VALUES('rebuild')` for the three external-content FTS5 tables.
pub(crate) fn rebuild_fts(conn: &Connection) -> Result<(), Error> {
    let tri = crate::search::substring_enabled(conn)?;
    for fts in ["blocks_fts", "blocks_fts_tri", "pages_fts"] {
        if fts == "blocks_fts_tri" && !tri {
            continue;
        }
        conn.execute_batch(&format!("INSERT INTO {fts}({fts}) VALUES('rebuild')"))?;
    }
    Ok(())
}
