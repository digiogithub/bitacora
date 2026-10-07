//! [`EchoStore`]: a [`FileStore`] decorator that registers every write with the watcher's
//! [`EchoFilter`] **before** the bytes reach the disk (BIT-T-0341).
//!
//! The filter matches on `(path, content hash)`, so registering before the atomic rename means
//! the file-system event that the rename produces can never be reported as an external change,
//! however quickly the watcher debounces it.

use std::io;

use bitacora_core::editor::{FileStat, FileStore};
use bitacora_core::graph_path::GraphPath;
use bitacora_core::recycle::recycle_path;
use bitacora_watch::EchoFilter;

/// Wraps a store and feeds `echo` before each write or removal.
pub struct EchoStore<S> {
    inner: S,
    echo: EchoFilter,
}

impl<S> std::fmt::Debug for EchoStore<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EchoStore")
    }
}

impl<S: FileStore> EchoStore<S> {
    /// Decorates `inner`.
    pub fn new(inner: S, echo: EchoFilter) -> Self {
        Self { inner, echo }
    }
}

impl<S: FileStore> FileStore for EchoStore<S> {
    fn read(&self, path: &GraphPath) -> io::Result<Option<Vec<u8>>> {
        self.inner.read(path)
    }

    fn stat(&self, path: &GraphPath) -> io::Result<Option<FileStat>> {
        self.inner.stat(path)
    }

    fn write(&mut self, path: &GraphPath, bytes: &[u8]) -> io::Result<()> {
        // A failed write leaves a harmless entry: it only matches these exact bytes for a few
        // seconds.
        self.echo.record_bytes(path.as_str(), bytes);
        self.inner.write(path, bytes)
    }

    fn remove(&mut self, path: &GraphPath) -> io::Result<()> {
        self.echo.record_deleted(path);
        self.inner.remove(path)
    }

    fn recycle(&mut self, path: &GraphPath) -> io::Result<Option<GraphPath>> {
        // The move removes `path` (rename source) as far as the watcher can tell: register that
        // before the rename, like every other deletion, and keep the real atomic move of the
        // inner store instead of the copy + remove default.
        self.echo.record_deleted(path);
        self.inner.recycle(path)
    }

    fn unrecycle(&mut self, path: &GraphPath) -> io::Result<bool> {
        // The restored file appears at `path` with the recycled bytes (rename destination).
        if let Ok(Some(bytes)) = self.inner.read(&recycle_path(path)) {
            self.echo.record_bytes(path.as_str(), &bytes);
        }
        self.inner.unrecycle(path)
    }

    fn list(&self, dir: &GraphPath) -> io::Result<Vec<String>> {
        self.inner.list(dir)
    }

    fn cleanup_stale(&mut self) -> usize {
        self.inner.cleanup_stale()
    }

    fn take_warnings(&mut self) -> Vec<String> {
        self.inner.take_warnings()
    }
}
