//! Startup reconcile, cold build and incremental reindex (design §4.1, §4.2).
//!
//! `scan -> stat filter -> blake3 diff -> parse (rayon, pure) -> writer thread`. The watcher
//! itself lives in `bitacora-watch`; this module consumes its events as [`FsChange`].

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bitacora_config::EffectiveConfig;
use bitacora_core::date::Date;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::journal::journal_file_path;
use bitacora_core::naming::{derive_title, page_key};
use bitacora_core::scan::{
    PARSER_EXTENSIONS, ScannedFile, has_allowed_extension, is_ignored_path, parse_order, scan_graph,
};
use rayon::prelude::*;
use rusqlite::OptionalExtension;

use crate::Error;
use crate::index::{Index, RebuildKind};
use crate::normalize::NormalizeOptions;
use crate::parse::{PARSER_VERSION, ParseConfig, parse};
use crate::pool::ReaderPool;
use crate::replace::{FileInput, FileKind, WriteOptions};
use crate::writer::{IndexEvent, IndexWriter, Pending};

/// Files parsed per wave; priority is honoured at wave granularity.
const WAVE: usize = 128;

/// A change reported by the file watcher (`bitacora-watch` will produce these; debouncing, the
/// 500 ms unlink re-check and echo suppression happen there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsChange {
    /// A file was created or modified.
    Modified(GraphPath),
    /// A file was removed (already re-checked by the watcher).
    Deleted(GraphPath),
    /// A file moved.
    Renamed {
        /// Old path.
        from: GraphPath,
        /// New path.
        to: GraphPath,
    },
    /// The watcher lost events (overflow, `MustScanSubDirs`, remount): run the full reconcile.
    Overflow,
}

/// Settings of the indexer.
#[derive(Debug, Clone)]
pub struct IndexerOptions {
    /// Graph root on disk.
    pub graph_root: std::path::PathBuf,
    /// Effective config (the parse depends on it).
    pub config: EffectiveConfig,
    /// Today's date, to prioritise today's journal. `None` disables that priority.
    pub today: Option<Date>,
    /// Hash every file instead of trusting `(size, mtime_ns)` (design §4.1 step 4).
    pub paranoid_scan: bool,
    /// Search normalisation.
    pub normalize: NormalizeOptions,
}

impl IndexerOptions {
    /// Defaults for everything but the graph and its config.
    pub fn new(graph_root: impl Into<std::path::PathBuf>, config: EffectiveConfig) -> Self {
        Self {
            graph_root: graph_root.into(),
            config,
            today: None,
            paranoid_scan: false,
            normalize: NormalizeOptions::default(),
        }
    }
}

/// What a reconcile did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileStats {
    /// Files found by the walk (parser extensions only).
    pub scanned: usize,
    /// Skipped by the `(size, mtime_ns)` prefilter.
    pub skipped: usize,
    /// Hash equal: only size/mtime updated.
    pub touched: usize,
    /// Parsed and written.
    pub parsed: usize,
    /// Removed from the index (not on disk any more).
    pub deleted: usize,
    /// The cold-build fast path was used.
    pub cold_build: bool,
    /// Files that could not be indexed, with the reason.
    pub errors: Vec<(String, String)>,
    /// Paths in the order they were queued for parsing (priority order).
    pub parse_order: Vec<String>,
    /// Wall time of the reconcile.
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
struct DbFile {
    size: i64,
    mtime_ns: i64,
    hash: Vec<u8>,
    parser_version: i64,
}

enum Action {
    Skip,
    Touch { size: u64, mtime_ns: i64 },
    Replace(Box<FileInput>),
}

/// Owns the writer thread and drives reconcile and incremental updates.
#[derive(Debug)]
pub struct Indexer {
    writer: IndexWriter,
    readers: ReaderPool,
    opts: IndexerOptions,
    pool: rayon::ThreadPool,
    requested: Mutex<HashSet<String>>,
    pending_rebuild: Mutex<RebuildKind>,
}

impl Indexer {
    /// Take the index's write connection, start the writer thread (seeding the built-in pages)
    /// and prepare the parse pool.
    pub fn start(index: &Index, opts: IndexerOptions) -> Result<Self, Error> {
        let write_opts = WriteOptions {
            normalize: opts.normalize,
        };
        let writer = IndexWriter::spawn(index.take_writer()?, write_opts)?;
        let threads = std::thread::available_parallelism()
            .map_or(2, std::num::NonZero::get)
            .saturating_sub(1)
            .max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("bitacora-parse-{i}"))
            .build()
            .map_err(|e| Error::io("<parse pool>", std::io::Error::other(e.to_string())))?;
        Ok(Self {
            writer,
            readers: index.readers().clone(),
            opts,
            pool,
            requested: Mutex::new(HashSet::new()),
            pending_rebuild: Mutex::new(index.outcome().rebuild),
        })
    }

    /// The writer, for components that submit their own jobs (editor, MCP).
    pub fn writer(&self) -> &IndexWriter {
        &self.writer
    }

    /// Receive index events.
    pub fn subscribe(&self) -> std::sync::mpsc::Receiver<IndexEvent> {
        self.writer.subscribe()
    }

    /// Ask for a page file to be parsed ahead of the queue (UI request, design §4.1 step 5).
    pub fn request_priority(&self, path: &GraphPath) {
        if let Ok(mut r) = self.requested.lock() {
            r.insert(path.as_str().to_owned());
        }
    }

    /// Stop the writer thread and hand the write connection back to the index.
    pub fn shutdown(self) {
        self.writer.shutdown();
    }

    fn parse_config(&self) -> ParseConfig<'_> {
        let mut pc = ParseConfig::new(&self.opts.config);
        pc.normalize = self.opts.normalize;
        pc
    }

    fn db_files(&self) -> Result<HashMap<String, DbFile>, Error> {
        let reader = self.readers.get()?;
        let mut stmt = reader
            .prepare("SELECT path, size, mtime_ns, content_hash, parser_version FROM files")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                DbFile {
                    size: r.get(1)?,
                    mtime_ns: r.get(2)?,
                    hash: r.get(3)?,
                    parser_version: r.get(4)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn db_file(&self, path: &str) -> Result<Option<DbFile>, Error> {
        let reader = self.readers.get()?;
        Ok(reader
            .query_row(
                "SELECT size, mtime_ns, content_hash, parser_version FROM files WHERE path = ?1",
                [path],
                |r| {
                    Ok(DbFile {
                        size: r.get(0)?,
                        mtime_ns: r.get(1)?,
                        hash: r.get(2)?,
                        parser_version: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    fn bulk_in_progress(&self) -> Result<bool, Error> {
        let reader = self.readers.get()?;
        Ok(reader
            .query_row(
                "SELECT 1 FROM meta WHERE key = 'bulk_in_progress'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// The full reconcile (design §4.1): walk, delete vanished files, skip unchanged ones, parse
    /// the rest by priority and write them. Uses the cold-build fast path on an empty or
    /// invalidated index. Records the parser/normalizer/config versions when it finishes.
    pub fn reconcile(&self) -> Result<ReconcileStats, Error> {
        let started = Instant::now();
        let mut stats = ReconcileStats::default();
        let root = &self.opts.graph_root;

        let scanned = scan_graph(root, &self.opts.config)?;
        let on_disk = parse_order(&scanned);
        stats.scanned = on_disk.len();

        let rebuild = *self
            .pending_rebuild
            .lock()
            .map_err(|_| Error::WriterStopped)?;
        let mut db = self.db_files()?;
        let interrupted = self.bulk_in_progress()?;
        let full = interrupted || rebuild == RebuildKind::FullReparse;
        let mut bulk = false;
        if full {
            // New, recreated, version-bumped or half-built index: start from nothing.
            stats.cold_build = true;
            self.writer.begin_bulk(interrupted || !db.is_empty())?;
            bulk = true;
            db.clear();
        } else if rebuild == RebuildKind::FtsOnly {
            self.writer.renormalize_search()?;
        }

        let result = self.reconcile_inner(&on_disk, db, &mut stats);
        if bulk {
            // Always leave bulk mode so triggers and foreign keys are restored.
            let end = self.writer.end_bulk();
            result?;
            end?;
        } else {
            result?;
        }
        if !stats.cold_build {
            self.refresh_duplicates()?;
        }
        self.writer.record_versions()?;
        self.writer
            .set_meta("last_full_scan_at", &now_ms().to_string())?;
        if let Ok(mut r) = self.pending_rebuild.lock() {
            *r = RebuildKind::None;
        }
        stats.elapsed = started.elapsed();
        Ok(stats)
    }

    fn reconcile_inner(
        &self,
        on_disk: &[ScannedFile],
        mut db: HashMap<String, DbFile>,
        stats: &mut ReconcileStats,
    ) -> Result<(), Error> {
        // Deleted files (db - disk).
        let disk: HashSet<&str> = on_disk.iter().map(|f| f.path.as_str()).collect();
        let gone: Vec<String> = db
            .keys()
            .filter(|p| !disk.contains(p.as_str()))
            .cloned()
            .collect();
        let mut tickets = Vec::new();
        for p in &gone {
            tickets.push(self.writer.submit_delete(p));
        }
        for t in tickets {
            if t.wait()?.is_some() {
                stats.deleted += 1;
            }
        }

        // Stat prefilter (parallel).
        let paranoid = self.opts.paranoid_scan;
        let candidates: Vec<&ScannedFile> = self.pool.install(|| {
            on_disk
                .par_iter()
                .filter(|f| {
                    let Some(row) = db.get(f.path.as_str()) else {
                        return true;
                    };
                    if paranoid || row.parser_version != i64::from(PARSER_VERSION) {
                        return true;
                    }
                    match stat(&f.abs) {
                        Ok((size, mtime)) => {
                            !(i64::try_from(size).ok() == Some(row.size) && mtime == row.mtime_ns)
                        }
                        Err(_) => true,
                    }
                })
                .collect()
        });
        stats.skipped = on_disk.len() - candidates.len();
        // Free the rows of skipped files early; candidates keep theirs for the hash check.
        db.retain(|p, _| candidates.iter().any(|c| c.path.as_str() == p));

        let ordered = self.prioritise(candidates);
        stats.parse_order = ordered.iter().map(|f| f.path.as_str().to_owned()).collect();

        let mut in_flight: Vec<(String, Pending<crate::ReplaceOutcome>)> = Vec::new();
        let mut touches: Vec<Pending<()>> = Vec::new();
        for wave in wave_chunks(&ordered, self.first_wave_len(&ordered)) {
            let loaded: Vec<(String, Result<Action, Error>)> = self.pool.install(|| {
                wave.par_iter()
                    .map(|f| {
                        let row = db.get(f.path.as_str());
                        (
                            f.path.as_str().to_owned(),
                            self.evaluate(&f.path, &f.abs, row, paranoid),
                        )
                    })
                    .collect()
            });
            // Wait for the previous wave only after this one was queued, so parsing the next
            // wave overlaps with writing this one.
            let previous = std::mem::take(&mut in_flight);
            for (path, action) in loaded {
                match action {
                    Ok(Action::Skip) => stats.skipped += 1,
                    Ok(Action::Touch { size, mtime_ns }) => {
                        stats.touched += 1;
                        touches.push(self.writer.touch_file(&path, size, mtime_ns));
                    }
                    Ok(Action::Replace(input)) => {
                        in_flight.push((path, self.writer.submit_replace(*input)));
                    }
                    Err(e) => stats.errors.push((path, e.to_string())),
                }
            }
            collect(previous, stats);
        }
        collect(std::mem::take(&mut in_flight), stats);
        for t in touches {
            t.wait()?;
        }
        Ok(())
    }

    fn first_wave_len(&self, ordered: &[&ScannedFile]) -> usize {
        // Today's journal, the home page and UI-requested files form the first (small) wave.
        ordered
            .iter()
            .take_while(|f| self.rank(f) <= 1)
            .count()
            .max(1)
    }

    fn rank(&self, f: &ScannedFile) -> u8 {
        let p = f.path.as_str();
        if let Some(today) = self.opts.today
            && journal_file_path(&self.opts.config, today).is_some_and(|j| j == f.path)
        {
            return 0;
        }
        if let Some(home) = self
            .opts
            .config
            .default_home()
            .and_then(|h| h.page)
            .filter(|h| !h.is_empty())
            && page_key(&derive_title(p, None, &self.opts.config)) == page_key(&home)
        {
            return 0;
        }
        if self.requested.lock().is_ok_and(|r| r.contains(p)) {
            return 1;
        }
        if p.starts_with(&format!("{}/", self.opts.config.journals_directory())) {
            return 2;
        }
        3
    }

    /// Order by priority: today/home, requested, journals newest first, then pages in Logseq's
    /// parse order.
    fn prioritise<'a>(&self, files: Vec<&'a ScannedFile>) -> Vec<&'a ScannedFile> {
        let mut keyed: Vec<(u8, usize, &ScannedFile)> = files
            .into_iter()
            .enumerate()
            .map(|(i, f)| (self.rank(f), i, f))
            .collect();
        // `files` arrives in `parse_order`, which already lists journals newest first, then the
        // rest; a stable sort by rank keeps those orders inside each rank.
        keyed.sort_by_key(|(rank, i, _)| (*rank, *i));
        keyed.into_iter().map(|(_, _, f)| f).collect()
    }

    /// Stat, read, hash and (when the content changed) parse one file. Pure with respect to the
    /// database; used by the parallel waves and by single-file updates.
    fn evaluate(
        &self,
        path: &GraphPath,
        abs: &Path,
        row: Option<&DbFile>,
        paranoid: bool,
    ) -> Result<Action, Error> {
        self.evaluate_forced(path, abs, row, paranoid, false)
    }

    fn evaluate_forced(
        &self,
        path: &GraphPath,
        abs: &Path,
        row: Option<&DbFile>,
        paranoid: bool,
        force: bool,
    ) -> Result<Action, Error> {
        let (size, mtime_ns) = stat(abs).map_err(|e| read_err(abs, e))?;
        let birth_ns = std::fs::metadata(abs)
            .ok()
            .and_then(|m| m.created().ok())
            .and_then(to_ns);
        if let Some(row) = row
            && !paranoid
            && row.parser_version == i64::from(PARSER_VERSION)
            && i64::try_from(size).ok() == Some(row.size)
            && mtime_ns == row.mtime_ns
        {
            return Ok(Action::Skip);
        }
        let bytes = std::fs::read(abs).map_err(|e| read_err(abs, e))?;
        let hash = blake3::hash(&bytes);
        if let Some(row) = row
            && !force
            && row.parser_version == i64::from(PARSER_VERSION)
            && row.hash.as_slice() == hash.as_bytes()
        {
            return Ok(Action::Touch { size, mtime_ns });
        }
        let parsed = parse(path, &bytes, &self.parse_config());
        Ok(Action::Replace(Box::new(FileInput {
            path: path.clone(),
            kind: FileKind::classify(path, &self.opts.config),
            size,
            mtime_ns,
            birth_ns,
            parsed,
        })))
    }

    /// Whether a path is something the index tracks (parser extension, not ignored or hidden).
    fn is_indexable(&self, path: &GraphPath) -> bool {
        let p = path.as_str();
        !is_ignored_path(p)
            && has_allowed_extension(p)
            && !self.opts.config.is_hidden(p)
            && path
                .extension()
                .is_some_and(|e| PARSER_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    }

    /// Apply one watcher event (design §4.2). Returns the reconcile stats for
    /// [`FsChange::Overflow`].
    pub fn handle(&self, change: &FsChange) -> Result<Option<ReconcileStats>, Error> {
        match change {
            FsChange::Overflow => return self.reconcile().map(Some),
            FsChange::Modified(p) => self.update_path(p)?,
            FsChange::Deleted(p) => {
                // The watcher re-checked, but be defensive: a file that is back is a modify.
                if p.to_fs_path(&self.opts.graph_root).is_file() {
                    self.update_path(p)?;
                } else {
                    self.writer.delete_file(p.as_str())?;
                    self.refresh_duplicates()?;
                }
            }
            FsChange::Renamed { from, to } => self.rename(from, to)?,
        }
        Ok(None)
    }

    /// Re-evaluate the files flagged `duplicate_page` whose page lost its owner (or has one with a
    /// larger path): the file with the smallest path takes the page over.
    pub fn refresh_duplicates(&self) -> Result<(), Error> {
        let paths: Vec<String> = {
            let reader = self.readers.get()?;
            let mut stmt = reader.prepare(
                "SELECT DISTINCT f.path FROM files f
                 JOIN blocks b ON b.file_id = f.id JOIN pages p ON p.id = b.page_id
                 LEFT JOIN files o ON o.id = p.file_id
                 WHERE f.status = 'duplicate_page' AND (p.file_id IS NULL OR o.path > f.path)
                 ORDER BY f.path",
            )?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        for p in paths {
            let path = GraphPath::new(&p)?;
            let abs = path.to_fs_path(&self.opts.graph_root);
            if !abs.is_file() {
                continue;
            }
            let row = self.db_file(&p)?;
            if let Action::Replace(input) =
                self.evaluate_forced(&path, &abs, row.as_ref(), true, true)?
            {
                self.writer.replace_file(*input)?;
            }
        }
        Ok(())
    }

    /// Re-read one path: delete it when gone, otherwise stat/hash and replace when it changed.
    pub fn update_path(&self, path: &GraphPath) -> Result<(), Error> {
        let abs = path.to_fs_path(&self.opts.graph_root);
        if !abs.is_file() {
            self.writer.delete_file(path.as_str())?;
            self.refresh_duplicates()?;
            return Ok(());
        }
        if !self.is_indexable(path) {
            return Ok(());
        }
        let row = self.db_file(path.as_str())?;
        // A watcher event means "something changed": hash instead of trusting (size, mtime).
        match self.evaluate(path, &abs, row.as_ref(), true)? {
            Action::Skip => {}
            Action::Touch { size, mtime_ns } => {
                self.writer
                    .touch_file(path.as_str(), size, mtime_ns)
                    .wait()?;
            }
            Action::Replace(input) => {
                self.writer.replace_file(*input)?;
                self.refresh_duplicates()?;
            }
        }
        Ok(())
    }

    /// Rename by hash: when `to` has the content that was indexed for `from`, move the row and
    /// reparse `to` with carry-over so block UUIDs survive; otherwise delete + modify.
    fn rename(&self, from: &GraphPath, to: &GraphPath) -> Result<(), Error> {
        let abs = to.to_fs_path(&self.opts.graph_root);
        if !abs.is_file() || !self.is_indexable(to) {
            self.writer.delete_file(from.as_str())?;
            return self.update_path(to);
        }
        let Some(row) = self.db_file(from.as_str())? else {
            return self.update_path(to);
        };
        let bytes = std::fs::read(&abs).map_err(|e| read_err(&abs, e))?;
        let same = row.hash.as_slice() == blake3::hash(&bytes).as_bytes();
        if !same {
            self.writer.delete_file(from.as_str())?;
            return self.update_path(to);
        }
        let (size, mtime_ns) = stat(&abs).map_err(|e| read_err(&abs, e))?;
        let birth_ns = std::fs::metadata(&abs)
            .ok()
            .and_then(|m| m.created().ok())
            .and_then(to_ns);
        let input = FileInput {
            path: to.clone(),
            kind: FileKind::classify(to, &self.opts.config),
            size,
            mtime_ns,
            birth_ns,
            parsed: parse(to, &bytes, &self.parse_config()),
        };
        self.writer.rename_file(from.as_str(), to.as_str(), input)?;
        self.refresh_duplicates()
    }
}

fn collect(batch: Vec<(String, Pending<crate::ReplaceOutcome>)>, stats: &mut ReconcileStats) {
    for (path, ticket) in batch {
        match ticket.wait() {
            Ok(_) => stats.parsed += 1,
            Err(e) => stats.errors.push((path, e.to_string())),
        }
    }
}

fn wave_chunks<'a, 'b>(
    ordered: &'b [&'a ScannedFile],
    first: usize,
) -> impl Iterator<Item = &'b [&'a ScannedFile]> {
    let first = first.min(ordered.len());
    let (head, tail) = ordered.split_at(first);
    std::iter::once(head)
        .filter(|h| !h.is_empty())
        .chain(tail.chunks(WAVE))
}

fn read_err(path: &Path, source: std::io::Error) -> Error {
    Error::ReadFile {
        path: path.to_owned(),
        source,
    }
}

fn to_ns(t: std::time::SystemTime) -> Option<i64> {
    let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_nanos()).ok()
}

fn stat(path: &Path) -> std::io::Result<(u64, i64)> {
    let m = std::fs::metadata(path)?;
    let mtime = m.modified().ok().and_then(to_ns).unwrap_or(0);
    Ok((m.len(), mtime))
}

fn now_ms() -> i64 {
    to_ns(std::time::SystemTime::now()).map_or(0, |n| n / 1_000_000)
}
