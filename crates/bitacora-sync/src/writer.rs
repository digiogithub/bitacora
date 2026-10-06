//! The seam between the sync engine and the graph's single writer (AGENTS.md rule 3, ADR-009).
//!
//! The sync engine never writes graph files itself. Everything that must change the work tree
//! (fast-forward, merge results, resolved conflicts) goes through a [`GraphWriter`], which
//! `bitacora-core` implements on top of its command queue once that exists (BIT-US-0062). The
//! engine only talks to git objects and refs directly.
//!
//! Protocol: [`GraphWriter::acquire`] flushes editors and pending writes, then blocks new writes
//! until the returned [`GraphLock`] is dropped. While the lock is held the engine stages, commits
//! and applies [`FileChange`]s through [`GraphLock::apply`].

use std::path::Path;

/// One work-tree mutation requested by the sync engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    /// Create or replace a file.
    Write {
        /// Graph-relative path using `/`.
        path: String,
        /// New content (already final: never contains conflict markers).
        content: Vec<u8>,
        /// Content the engine believes is on disk (`None`: the file must not exist). The writer
        /// MUST refuse with [`WriterError::Stale`] when the on-disk bytes differ, so a file the
        /// user touched since the engine read it is never overwritten (AGENTS.md rule 1).
        expected: Option<Vec<u8>>,
    },
    /// Remove a file.
    Delete {
        /// Graph-relative path using `/`.
        path: String,
        /// Content the engine believes is on disk; same rule as for `Write`.
        expected: Option<Vec<u8>>,
    },
}

impl FileChange {
    /// The affected path.
    pub fn path(&self) -> &str {
        match self {
            Self::Write { path, .. } | Self::Delete { path, .. } => path,
        }
    }
}

/// Errors reported by a [`GraphWriter`].
#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    /// A write transaction is pending or editors could not be flushed; try again later.
    #[error("graph writer is busy")]
    Busy,
    /// A file changed on disk since the engine read it; nothing was applied.
    #[error("`{0}` changed on disk since it was read")]
    Stale(String),
    /// The writer failed for another reason.
    #[error("graph writer failed: {0}")]
    Failed(String),
}

/// Exclusive access to the graph while the sync engine works.
pub trait GraphLock {
    /// Applies `changes` atomically per file (temp file + fsync + rename in the real writer),
    /// updating the index and notifying editors. All-or-nothing on [`WriterError::Stale`]: the
    /// writer checks every `expected` first and applies nothing when one differs.
    fn apply(&mut self, changes: &[FileChange]) -> Result<(), WriterError>;
}

/// The single writer of the graph folder.
pub trait GraphWriter: Send + Sync {
    /// Flushes open editors and pending writes (`flush_all`), then returns the exclusive lock.
    /// Returns [`WriterError::Busy`] instead of blocking when a write transaction is in progress
    /// and cannot finish promptly; the engine retries later.
    fn acquire(&self) -> Result<Box<dyn GraphLock + '_>, WriterError>;
}

/// Test doubles. Compiled in all builds because integration tests (and a headless CLI until
/// core's writer lands) use them; they are not part of the stable API.
pub mod testing {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::{FileChange, GraphLock, GraphWriter, WriterError};

    /// A writer that applies changes directly to a directory, standing in for the core writer in
    /// tests. It enforces the `expected` contract like the real one.
    #[derive(Debug)]
    pub struct DirGraphWriter {
        root: PathBuf,
        busy: AtomicBool,
        acquires: AtomicUsize,
        applied: Mutex<Vec<FileChange>>,
        mutex: Mutex<()>,
    }

    impl DirGraphWriter {
        /// Writer for the graph at `root`.
        pub fn new(root: &Path) -> Self {
            Self {
                root: root.to_path_buf(),
                busy: AtomicBool::new(false),
                acquires: AtomicUsize::new(0),
                applied: Mutex::new(Vec::new()),
                mutex: Mutex::new(()),
            }
        }

        /// While `true`, `acquire` reports [`WriterError::Busy`] (a pending write transaction).
        pub fn set_busy(&self, busy: bool) {
            self.busy.store(busy, Ordering::SeqCst);
        }

        /// How many successful `acquire` calls happened.
        pub fn acquire_count(&self) -> usize {
            self.acquires.load(Ordering::SeqCst)
        }

        /// Every change applied so far.
        pub fn applied(&self) -> Vec<FileChange> {
            self.applied
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    struct DirLock<'a> {
        writer: &'a DirGraphWriter,
        _guard: std::sync::MutexGuard<'a, ()>,
    }

    impl GraphWriter for DirGraphWriter {
        fn acquire(&self) -> Result<Box<dyn GraphLock + '_>, WriterError> {
            if self.busy.load(Ordering::SeqCst) {
                return Err(WriterError::Busy);
            }
            let guard = self
                .mutex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.acquires.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(DirLock {
                writer: self,
                _guard: guard,
            }))
        }
    }

    impl GraphLock for DirLock<'_> {
        fn apply(&mut self, changes: &[FileChange]) -> Result<(), WriterError> {
            let root = &self.writer.root;
            let io = |e: std::io::Error| WriterError::Failed(e.to_string());
            for change in changes {
                let (path, expected) = match change {
                    FileChange::Write { path, expected, .. }
                    | FileChange::Delete { path, expected } => (path, expected),
                };
                let on_disk = std::fs::read(root.join(path)).ok();
                if on_disk != *expected {
                    return Err(WriterError::Stale(path.clone()));
                }
            }
            for change in changes {
                match change {
                    FileChange::Write { path, content, .. } => {
                        let full = root.join(path);
                        if let Some(parent) = full.parent() {
                            std::fs::create_dir_all(parent).map_err(io)?;
                        }
                        let mut tmp_name = full.as_os_str().to_owned();
                        tmp_name.push(".bitacora-tmp");
                        let tmp = PathBuf::from(tmp_name);
                        std::fs::write(&tmp, content).map_err(io)?;
                        std::fs::rename(&tmp, &full).map_err(io)?;
                    }
                    FileChange::Delete { path, .. } => {
                        std::fs::remove_file(root.join(path)).map_err(io)?;
                    }
                }
            }
            self.writer
                .applied
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend(changes.iter().cloned());
            Ok(())
        }
    }
}

/// Graph-relative path check shared by the engine: never touch git metadata or escape the graph.
pub(crate) fn is_safe_relative(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty()
        && p.is_relative()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        && !path.starts_with(".git/")
        && path != ".git"
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::testing::DirGraphWriter;
    use super::*;

    #[test]
    fn stale_expectation_applies_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "user edit").unwrap();
        let w = DirGraphWriter::new(dir.path());
        let mut lock = w.acquire().unwrap();
        let err = lock
            .apply(&[
                FileChange::Write {
                    path: "b.md".into(),
                    content: b"x".to_vec(),
                    expected: None,
                },
                FileChange::Write {
                    path: "a.md".into(),
                    content: b"theirs".to_vec(),
                    expected: Some(b"old".to_vec()),
                },
            ])
            .unwrap_err();
        assert!(matches!(err, WriterError::Stale(p) if p == "a.md"));
        assert!(!dir.path().join("b.md").exists());
        assert_eq!(
            std::fs::read(dir.path().join("a.md")).unwrap(),
            b"user edit"
        );
    }

    #[test]
    fn busy_writer_refuses_acquire() {
        let dir = tempfile::tempdir().unwrap();
        let w = DirGraphWriter::new(dir.path());
        w.set_busy(true);
        assert!(matches!(w.acquire(), Err(WriterError::Busy)));
    }

    #[test]
    fn safe_paths() {
        assert!(is_safe_relative("pages/A.md"));
        assert!(!is_safe_relative("../x"));
        assert!(!is_safe_relative("/abs"));
        assert!(!is_safe_relative(".git/config"));
        assert!(!is_safe_relative(""));
    }
}
