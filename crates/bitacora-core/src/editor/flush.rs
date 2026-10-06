//! Writing dirty pages: the seam to the file system (AGENTS.md rules 1, 3, 4).
//!
//! [`FileStore`] is the only way core touches graph files. [`FsStore`] is the real one (temp
//! file in the same directory, fsync, rename). The debounced write queue (BIT-US-0063), the
//! hash-based pre-write merge (BIT-US-0065) and the self-check/backup extras (BIT-US-0064/0066)
//! plug in around [`Workspace::flush`]: today a page whose file changed on disk is reported as a
//! conflict and left dirty, never overwritten.

use std::collections::BTreeMap;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::workspace::Workspace;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;

/// Reads and writes graph files by graph-relative path.
pub trait FileStore {
    /// Current bytes of a file, `None` when it does not exist.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    fn read(&self, path: &GraphPath) -> io::Result<Option<Vec<u8>>>;
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
}

/// File store over a directory. Writes are atomic: `.<name>.bitacora-tmp` + fsync + rename,
/// keeping the permissions of the replaced file. Files are written as given (callers produce
/// UTF-8/LF for new files and keep the existing line endings for edited ones).
#[derive(Debug, Clone)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    /// Store rooted at the graph folder.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn abs(&self, p: &GraphPath) -> PathBuf {
        p.to_fs_path(&self.root)
    }
}

impl FileStore for FsStore {
    fn read(&self, path: &GraphPath) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.abs(path)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&mut self, path: &GraphPath, bytes: &[u8]) -> io::Result<()> {
        let target = self.abs(path);
        let dir: &Path = target.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(dir)?;
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = dir.join(format!(".{name}.bitacora-tmp"));
        let result = (|| {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            drop(f);
            if let Ok(meta) = std::fs::metadata(&target) {
                std::fs::set_permissions(&tmp, meta.permissions())?;
            }
            std::fs::rename(&tmp, &target)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    fn remove(&mut self, path: &GraphPath) -> io::Result<()> {
        match std::fs::remove_file(self.abs(path)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
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
    /// Pages left dirty because their file changed on disk since we read it.
    pub conflicts: Vec<PageKey>,
    /// Pages left dirty because of I/O errors (page, message).
    pub failed: Vec<(PageKey, String)>,
}

impl FlushReport {
    /// Nothing is left to write.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.conflicts.is_empty() && self.failed.is_empty()
    }
}

impl Workspace {
    /// Writes every page that needs it and removes the files of deleted pages. A page whose
    /// file no longer holds the bytes we last read is **not** written (it is reported in
    /// `conflicts`).
    pub fn flush(&mut self, store: &mut dyn FileStore) -> FlushReport {
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
                        Err(e) => report
                            .failed
                            .push((PageKey::from_title(path.as_str()), e.to_string())),
                    }
                }
                Ok(Some(_)) => report.conflicts.push(PageKey::from_title(path.as_str())),
                Err(e) => report
                    .failed
                    .push((PageKey::from_title(path.as_str()), e.to_string())),
            }
        }
        for key in self.dirty_pages() {
            self.flush_page(&key, store, &mut report);
        }
        report
    }

    fn flush_page(&mut self, key: &PageKey, store: &mut dyn FileStore, report: &mut FlushReport) {
        let Some(page) = self.page(key) else { return };
        let Some(path) = page.path.clone() else {
            return;
        };
        let (old_path, base) = (page.disk_path.clone(), page.disk.clone());
        let bytes = page.serialize();
        // Pre-write check: the file must still hold what we last read or wrote.
        let check_path = old_path.as_ref().unwrap_or(&path);
        let on_disk = match store.read(check_path) {
            Ok(b) => b,
            Err(e) => {
                report.failed.push((key.clone(), e.to_string()));
                return;
            }
        };
        let expected = base.as_ref().map(|d| &d.bytes[..]);
        if on_disk.as_deref() != expected && !(on_disk.is_none() && expected.is_some()) {
            report.conflicts.push(key.clone());
            return;
        }
        // A rename target that already exists is never overwritten.
        if old_path.as_ref().is_some_and(|o| *o != path) {
            match store.read(&path) {
                Ok(None) => {}
                Ok(Some(_)) => {
                    report.conflicts.push(key.clone());
                    return;
                }
                Err(e) => {
                    report.failed.push((key.clone(), e.to_string()));
                    return;
                }
            }
        }
        let unchanged = on_disk.as_deref() == Some(&bytes[..]) && old_path.as_ref() == Some(&path);
        if !unchanged && let Err(e) = store.write(&path, &bytes) {
            report.failed.push((key.clone(), e.to_string()));
            return;
        }
        if let Some(old) = old_path.filter(|o| *o != path)
            && let Err(e) = store.remove(&old)
        {
            report.failed.push((key.clone(), e.to_string()));
        }
        if let Some(p) = self.pages_mut().get_mut(key) {
            match p.mark_saved(&bytes) {
                Ok(()) => {
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
}
