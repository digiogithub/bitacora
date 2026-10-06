//! The real file system behind [`FileStore`]: atomic, crash-safe writes (BIT-US-0064,
//! BIT-SP-0005.R3; AGENTS.md rule 4).
//!
//! Sequence of [`atomic_write`]: `.<name>.bitacora-tmp` in the same directory -> write ->
//! `fsync` -> copy permissions -> `rename` over the target -> `fsync` of the directory (Unix).
//! At any instant a reader or a crash sees either the complete old file or the complete new one.
//!
//! * Windows: `std::fs::rename` is `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, which replaces an
//!   existing file in one step; directories cannot be fsynced there and the step is skipped.
//! * File systems that refuse the rename (FAT shares, some FUSE mounts) get an in-place rewrite
//!   as a last resort; that is reported as a warning because it is not crash-safe.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::flush::{FileStat, FileStore};
use crate::graph_path::GraphPath;
use crate::recycle::recycle_path;

/// Suffix of the temporary file next to each target.
pub const TMP_SUFFIX: &str = ".bitacora-tmp";

/// How [`atomic_write`] stored the bytes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WriteMode {
    /// Temp file + rename.
    Atomic,
    /// The file system refused the rename; the target was rewritten in place (not crash-safe).
    InPlace,
}

fn tmp_path(target: &Path) -> PathBuf {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dir.join(format!(".{name}{TMP_SUFFIX}"))
}

#[cfg(unix)]
fn sync_dir(dir: &Path) {
    // Best effort: some file systems cannot fsync a directory; the data itself is synced.
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) {}

fn rename_unsupported(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
    ) || matches!(e.raw_os_error(), Some(1 | 38 | 95))
    // EPERM, ENOSYS, EOPNOTSUPP
}

/// Atomically replaces (or creates) `target` with `bytes`, creating missing parent directories.
/// Permissions of an existing target are kept.
///
/// # Errors
/// I/O errors; the target is then untouched (and the temp file removed) unless the in-place
/// fallback was taken.
pub fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<WriteMode> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = tmp_path(target);
    let staged = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if let Ok(meta) = std::fs::metadata(target) {
            std::fs::set_permissions(&tmp, meta.permissions())?;
        }
        Ok::<(), io::Error>(())
    })();
    if let Err(e) = staged {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    match std::fs::rename(&tmp, target) {
        Ok(()) => {
            sync_dir(dir);
            Ok(WriteMode::Atomic)
        }
        Err(e) if rename_unsupported(&e) && target.is_file() => {
            let _ = std::fs::remove_file(&tmp);
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(target)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            Ok(WriteMode::InPlace)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Moves `from` to `to`, replacing an existing `to`, creating parent directories. Falls back to
/// an atomic copy plus removal when the rename crosses file systems.
fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => {
            if let Some(d) = from.parent() {
                sync_dir(d);
            }
            if let Some(d) = to.parent() {
                sync_dir(d);
            }
            Ok(())
        }
        Err(_) => {
            let bytes = std::fs::read(from)?;
            atomic_write(to, &bytes)?;
            std::fs::remove_file(from)
        }
    }
}

/// Removes leftover `.<name>.bitacora-tmp` files (a crash between create and rename) below
/// `root`, skipping `.git` and `logseq/bak`. Returns how many were removed.
#[must_use]
pub fn cleanup_stale_tmp(root: &Path) -> usize {
    let mut removed = 0;
    let walker = walkdir::WalkDir::new(root).into_iter().filter_entry(|e| {
        let n = e.file_name().to_string_lossy();
        !(e.depth() > 0 && e.file_type().is_dir() && (n == ".git" || n == "node_modules"))
    });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.')
            && name.ends_with(TMP_SUFFIX)
            && std::fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// File store over a directory (the graph folder). Files are written as given: callers produce
/// UTF-8/LF/BOM-free bytes for new files and keep the existing line endings for edited ones.
#[derive(Debug, Clone)]
pub struct FsStore {
    root: PathBuf,
    warnings: Vec<String>,
}

impl FsStore {
    /// Store rooted at the graph folder.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            warnings: Vec::new(),
        }
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

    fn stat(&self, path: &GraphPath) -> io::Result<Option<FileStat>> {
        match std::fs::metadata(self.abs(path)) {
            Ok(m) if m.is_dir() => Err(io::Error::other(format!("`{path}` is a directory"))),
            Ok(m) => Ok(Some(FileStat {
                len: m.len(),
                mtime: m.modified().ok(),
            })),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&mut self, path: &GraphPath, bytes: &[u8]) -> io::Result<()> {
        if atomic_write(&self.abs(path), bytes)? == WriteMode::InPlace {
            self.warnings.push(format!(
                "`{path}`: the file system refused an atomic replace; wrote in place"
            ));
        }
        Ok(())
    }

    fn remove(&mut self, path: &GraphPath) -> io::Result<()> {
        match std::fs::remove_file(self.abs(path)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn recycle(&mut self, path: &GraphPath) -> io::Result<Option<GraphPath>> {
        let from = self.abs(path);
        if !from.is_file() {
            return Ok(None);
        }
        let dest = recycle_path(path);
        move_file(&from, &self.abs(&dest))?;
        Ok(Some(dest))
    }

    fn unrecycle(&mut self, path: &GraphPath) -> io::Result<bool> {
        let to = self.abs(path);
        let src = self.abs(&recycle_path(path));
        if to.exists() || !src.is_file() {
            return Ok(false);
        }
        move_file(&src, &to)?;
        Ok(true)
    }

    fn list(&self, dir: &GraphPath) -> io::Result<Vec<String>> {
        let rd = match std::fs::read_dir(self.abs(dir)) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        for e in rd {
            let e = e?;
            if e.file_type()?.is_file() {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        Ok(out)
    }

    fn cleanup_stale(&mut self) -> usize {
        cleanup_stale_tmp(&self.root)
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Modification time helper for tests and callers that stat through std.
#[must_use]
pub fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}
