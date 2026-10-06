//! Writing dirty pages: the seam to the file system (AGENTS.md rules 1, 3, 4).
//!
//! [`FileStore`] is the only way core touches graph files; [`super::fsio::FsStore`] is the real
//! one (atomic writes). [`Workspace::flush`] implements the write protocol of
//! `docs/design/block-editor.md` §5.2:
//!
//! 1. serialize with the self-check (a page that cannot be rendered faithfully is never written);
//! 2. pre-write check against the in-memory [`DiskSnapshot`](super::model::DiskSnapshot):
//!    `stat`, and only when `(len, mtime)` differ read and hash. A file that changed on disk is
//!    **never overwritten**: the page stays dirty and is reported in `conflicts`;
//! 3. a file that vanished while the page is dirty is recreated (reported in `recreated`);
//! 4. `logseq/bak` backup of the old content when the write removes text;
//! 5. atomic write, then the snapshot and all block origins are rebased onto the new bytes;
//! 6. failures keep the page dirty, try to save the new content to `logseq/bak` and are listed in
//!    `failed` / `unwritten`.

use std::collections::BTreeMap;
use std::io;
use std::time::SystemTime;

use super::backup;
use super::model::DiskSnapshot;
use super::workspace::Workspace;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;

pub use super::fsio::FsStore;

/// Size and modification time of a file, the cheap part of the pre-write check.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FileStat {
    /// Length in bytes.
    pub len: u64,
    /// Modification time, when the store knows it.
    pub mtime: Option<SystemTime>,
}

/// Reads and writes graph files by graph-relative path.
pub trait FileStore {
    /// Current bytes of a file, `None` when it does not exist.
    ///
    /// # Errors
    /// I/O errors other than "not found" (including "is a directory").
    fn read(&self, path: &GraphPath) -> io::Result<Option<Vec<u8>>>;
    /// Size and mtime, `None` when the file does not exist. The default reads the file.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn stat(&self, path: &GraphPath) -> io::Result<Option<FileStat>> {
        Ok(self.read(path)?.map(|b| FileStat {
            len: b.len() as u64,
            mtime: None,
        }))
    }
    /// Atomically replaces (or creates) a file.
    ///
    /// # Errors
    /// I/O errors.
    fn write(&mut self, path: &GraphPath, bytes: &[u8]) -> io::Result<()>;
    /// Removes a file (missing is fine).
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn remove(&mut self, path: &GraphPath) -> io::Result<()>;
    /// File names (not paths) directly inside `dir`; empty when it does not exist.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn list(&self, dir: &GraphPath) -> io::Result<Vec<String>>;
    /// Removes leftovers of interrupted writes; returns how many.
    fn cleanup_stale(&mut self) -> usize {
        0
    }
    /// Drains non-fatal notices (e.g. an in-place fallback) since the last call.
    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// In-memory store (tests, dry runs).
#[derive(Debug, Default, Clone)]
pub struct MemStore {
    /// Files by graph path.
    pub files: BTreeMap<GraphPath, Vec<u8>>,
}

impl FileStore for MemStore {
    fn read(&self, path: &GraphPath) -> io::Result<Option<Vec<u8>>> {
        Ok(self.files.get(path).cloned())
    }
    fn write(&mut self, path: &GraphPath, bytes: &[u8]) -> io::Result<()> {
        self.files.insert(path.clone(), bytes.to_vec());
        Ok(())
    }
    fn remove(&mut self, path: &GraphPath) -> io::Result<()> {
        self.files.remove(path);
        Ok(())
    }
    fn list(&self, dir: &GraphPath) -> io::Result<Vec<String>> {
        let prefix = format!("{}/", dir.as_str());
        Ok(self
            .files
            .keys()
            .filter_map(|p| p.as_str().strip_prefix(&prefix))
            .filter(|rest| !rest.contains('/'))
            .map(str::to_owned)
            .collect())
    }
}

/// A file Bitacora wrote (feeds the echo filter and the indexer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenFile {
    /// Page that was written (`None` for files written for sync).
    pub page: Option<PageKey>,
    /// Graph-relative path.
    pub path: GraphPath,
    /// BLAKE3 of the written bytes.
    pub hash: blake3::Hash,
}

/// Outcome of [`Workspace::flush`].
#[derive(Debug, Default, Clone)]
pub struct FlushReport {
    /// Files written.
    pub written: Vec<WrittenFile>,
    /// Files removed.
    pub deleted: Vec<GraphPath>,
    /// Pages left dirty because their file changed on disk since we read it (never overwritten).
    pub conflicts: Vec<PageKey>,
    /// Pages left dirty because of I/O or self-check errors (page, message).
    pub failed: Vec<(PageKey, String)>,
    /// Files that could not be written, for the "unsaved files" notice (multi-file
    /// transactions list every one).
    pub unwritten: Vec<GraphPath>,
    /// Dirty pages whose file had been deleted externally and was recreated.
    pub recreated: Vec<PageKey>,
    /// Pages for which the canonical render replaced the preferred one (a serializer bug to log).
    pub self_check_fallbacks: Vec<PageKey>,
    /// `logseq/bak` files written.
    pub backups: Vec<GraphPath>,
    /// Non-fatal notices (backup failed, in-place fallback, ...).
    pub warnings: Vec<String>,
}

impl FlushReport {
    /// Nothing is left to write.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.conflicts.is_empty() && self.failed.is_empty()
    }
}

/// What happened to a page whose external version was taken ([`Workspace::take_disk`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeDisk {
    /// The page was reloaded from disk (block ids changed).
    Reloaded,
    /// The file no longer exists, the page was removed.
    Removed,
    /// The page is not loaded.
    Unknown,
}

fn fail(report: &mut FlushReport, key: &PageKey, path: &GraphPath, msg: String) {
    report.failed.push((key.clone(), msg));
    if !report.unwritten.contains(path) {
        report.unwritten.push(path.clone());
    }
}

impl Workspace {
    /// Writes every page that needs it and removes the files of deleted pages. A page whose
    /// file no longer holds the bytes we last read is **not** written (it is reported in
    /// `conflicts`).
    pub fn flush(&mut self, store: &mut dyn FileStore) -> FlushReport {
        self.flush_at(store, SystemTime::now(), None)
    }

    /// Like [`Workspace::flush`] but only for `keys` (and the pending file deletions): the
    /// debounced write queue flushes the pages that are due.
    pub fn flush_pages(&mut self, store: &mut dyn FileStore, keys: &[PageKey]) -> FlushReport {
        self.flush_at(store, SystemTime::now(), Some(keys))
    }

    /// [`Workspace::flush`] with an explicit clock (backup names) and optional page filter.
    pub fn flush_at(
        &mut self,
        store: &mut dyn FileStore,
        now: SystemTime,
        only: Option<&[PageKey]>,
    ) -> FlushReport {
        let mut report = FlushReport::default();
        let deletes: Vec<_> = self
            .pending_deletes()
            .iter()
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();
        for (path, expected) in deletes {
            match store.read(&path) {
                Ok(None) => self.finish_delete(&path),
                Ok(Some(cur)) if expected.as_deref().is_none_or(|e| e == cur.as_slice()) => {
                    match store.remove(&path) {
                        Ok(()) => {
                            self.finish_delete(&path);
                            report.deleted.push(path);
                        }
                        Err(e) => {
                            let k = PageKey::from_title(path.as_str());
                            fail(&mut report, &k, &path, e.to_string());
                        }
                    }
                }
                Ok(Some(_)) => report.conflicts.push(PageKey::from_title(path.as_str())),
                Err(e) => {
                    let k = PageKey::from_title(path.as_str());
                    fail(&mut report, &k, &path, e.to_string());
                }
            }
        }
        let keys = self.dirty_pages();
        for key in keys {
            if only.is_some_and(|o| !o.contains(&key)) {
                continue;
            }
            self.flush_page(&key, store, &mut report, now);
        }
        report.warnings.extend(store.take_warnings());
        report
    }

    fn flush_page(
        &mut self,
        key: &PageKey,
        store: &mut dyn FileStore,
        report: &mut FlushReport,
        now: SystemTime,
    ) {
        let Some(page) = self.page(key) else { return };
        let Some(path) = page.path.clone() else {
            return;
        };
        let (old_path, base) = (page.disk_path.clone(), page.disk.clone());
        let ser = page.serialize_checked();
        if !ser.ok {
            // Refuse to write something that would re-parse differently from the model.
            fail(
                report,
                key,
                &path,
                "serializer self-check failed; the page was not written".into(),
            );
            return;
        }
        if ser.fell_back {
            report.self_check_fallbacks.push(key.clone());
        }
        let bytes = ser.bytes;
        let check_path = old_path.as_ref().unwrap_or(&path);

        // Pre-write check: stat, then hash only when (len, mtime) moved.
        let on_disk = match pre_write_state(store, check_path, base.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                fail(report, key, &path, e.to_string());
                return;
            }
        };
        let current: Option<Vec<u8>> = match on_disk {
            DiskState::Changed => {
                report.conflicts.push(key.clone());
                return;
            }
            DiskState::Missing => {
                if base.is_some() {
                    report.recreated.push(key.clone());
                }
                None
            }
            DiskState::Same(b) => b,
        };

        // A rename target that already exists is never overwritten.
        if old_path.as_ref().is_some_and(|o| *o != path) {
            match store.read(&path) {
                Ok(None) => {}
                Ok(Some(_)) => {
                    report.conflicts.push(key.clone());
                    return;
                }
                Err(e) => {
                    fail(report, key, &path, e.to_string());
                    return;
                }
            }
        }
        let unchanged = current.as_deref() == Some(&bytes[..]) && old_path.as_ref() == Some(&path);
        if !unchanged {
            if let Some(old) = &current
                && backup::removes_text(old, &bytes)
            {
                match backup::write_backup(store, check_path, old, now) {
                    Ok(b) => report.backups.push(b),
                    Err(e) => report
                        .warnings
                        .push(format!("backup of `{check_path}` failed: {e}")),
                }
            }
            if let Err(e) = store.write(&path, &bytes) {
                // Keep the new content somewhere safe and stay dirty.
                match backup::write_backup(store, &path, &bytes, now) {
                    Ok(b) => report.backups.push(b),
                    Err(be) => report
                        .warnings
                        .push(format!("could not back up unsaved `{path}`: {be}")),
                }
                fail(report, key, &path, e.to_string());
                return;
            }
        }
        if let Some(old) = old_path.filter(|o| *o != path)
            && let Err(e) = store.remove(&old)
        {
            report.failed.push((key.clone(), e.to_string()));
        }
        let stat = store.stat(&path).ok().flatten();
        if let Some(p) = self.pages_mut().get_mut(key) {
            match p.mark_saved(&bytes) {
                Ok(()) => {
                    p.set_disk_stat(stat);
                    if !unchanged {
                        report.written.push(WrittenFile {
                            page: Some(key.clone()),
                            path,
                            hash: blake3::hash(&bytes),
                        });
                    }
                }
                Err(e) => report.failed.push((key.clone(), e.to_string())),
            }
        }
    }

    /// "Keep mine": overwrites the external version of a conflicting page with our unsaved
    /// content. The external bytes are saved to `logseq/bak` first (BIT-SP-0005.R9), then the
    /// page is flushed against the file as it is now.
    pub fn resolve_keep_mine(
        &mut self,
        key: &PageKey,
        store: &mut dyn FileStore,
        now: SystemTime,
    ) -> FlushReport {
        let mut report = FlushReport::default();
        let Some(page) = self.page(key) else {
            return report;
        };
        let check_path = page.disk_path.clone().or_else(|| page.path.clone());
        if let Some(cp) = check_path {
            match store.read(&cp) {
                Ok(Some(ext)) => {
                    match backup::write_backup(store, &cp, &ext, now) {
                        Ok(b) => report.backups.push(b),
                        Err(e) => {
                            // Without the backup the overwrite would lose the external edit.
                            fail(
                                &mut report,
                                key,
                                &cp,
                                format!("could not back up the external version: {e}"),
                            );
                            return report;
                        }
                    }
                    if let Some(p) = self.pages_mut().get_mut(key) {
                        p.disk = Some(DiskSnapshot::new(&ext));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    fail(&mut report, key, &cp, e.to_string());
                    return report;
                }
            }
        }
        let rest = self.flush_at(store, now, Some(std::slice::from_ref(key)));
        merge_reports(&mut report, rest);
        report
    }

    /// "Take disk": replaces the unsaved content of a page by the external version. The unsaved
    /// content goes to `logseq/bak` first. Block ids of the page change.
    pub fn take_disk(
        &mut self,
        key: &PageKey,
        store: &mut dyn FileStore,
        now: SystemTime,
        report: &mut FlushReport,
    ) -> TakeDisk {
        let Some(page) = self.page(key) else {
            return TakeDisk::Unknown;
        };
        let title = page.title.clone();
        let Some(path) = page.disk_path.clone().or_else(|| page.path.clone()) else {
            return TakeDisk::Unknown;
        };
        let unsaved = page.needs_write().then(|| page.serialize());
        if let Some(u) = unsaved {
            match backup::write_backup(store, &path, &u, now) {
                Ok(b) => report.backups.push(b),
                Err(e) => report
                    .warnings
                    .push(format!("could not back up unsaved `{path}`: {e}")),
            }
        }
        match store.read(&path) {
            Ok(Some(bytes)) => {
                let stat = store.stat(&path).ok().flatten();
                self.load_page(key.clone(), &title, Some(path), &bytes);
                if let Some(p) = self.pages_mut().get_mut(key) {
                    p.set_disk_stat(stat);
                }
                TakeDisk::Reloaded
            }
            Ok(None) => {
                self.remove_page(key);
                TakeDisk::Removed
            }
            Err(e) => {
                fail(report, key, &path, e.to_string());
                TakeDisk::Unknown
            }
        }
    }

    /// Drops loaded pages whose file has been deleted externally and that have no unsaved edits
    /// (dirty pages are recreated by the next flush instead). Returns the removed pages; the
    /// caller removes them from the index.
    pub fn drop_missing(&mut self, store: &dyn FileStore, paths: &[GraphPath]) -> Vec<PageKey> {
        let mut gone = Vec::new();
        for p in paths {
            let Some(key) = self.page_for_path(p).cloned() else {
                continue;
            };
            let Some(page) = self.page(&key) else {
                continue;
            };
            if page.needs_write() {
                continue;
            }
            let disk = page.disk_path.clone().unwrap_or_else(|| p.clone());
            if matches!(store.stat(&disk), Ok(None)) {
                self.remove_page(&key);
                gone.push(key);
            }
        }
        gone
    }
}

fn merge_reports(into: &mut FlushReport, from: FlushReport) {
    into.written.extend(from.written);
    into.deleted.extend(from.deleted);
    into.conflicts.extend(from.conflicts);
    into.failed.extend(from.failed);
    into.unwritten.extend(from.unwritten);
    into.recreated.extend(from.recreated);
    into.self_check_fallbacks.extend(from.self_check_fallbacks);
    into.backups.extend(from.backups);
    into.warnings.extend(from.warnings);
}

enum DiskState {
    /// The file holds what we last read or wrote (bytes when they were read, `None` for a page
    /// never written whose file does not exist).
    Same(Option<Vec<u8>>),
    /// The file does not exist.
    Missing,
    /// The file differs from the snapshot.
    Changed,
}

fn pre_write_state(
    store: &dyn FileStore,
    path: &GraphPath,
    base: Option<&DiskSnapshot>,
) -> io::Result<DiskState> {
    let Some(stat) = store.stat(path)? else {
        // A page never written has no file: that is the expected state.
        return Ok(if base.is_some() {
            DiskState::Missing
        } else {
            DiskState::Same(None)
        });
    };
    let Some(snap) = base else {
        // We never saw this file but it exists now: somebody else created it.
        return Ok(DiskState::Changed);
    };
    if stat.mtime.is_some()
        && snap.mtime == stat.mtime
        && snap.len == Some(stat.len)
        && stat.len == snap.bytes.len() as u64
    {
        return Ok(DiskState::Same(Some(snap.bytes.to_vec())));
    }
    let Some(cur) = store.read(path)? else {
        return Ok(DiskState::Missing);
    };
    if blake3::hash(&cur) == snap.hash {
        Ok(DiskState::Same(Some(cur)))
    } else {
        Ok(DiskState::Changed)
    }
}
