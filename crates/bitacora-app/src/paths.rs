//! Platform directories for the app (config, data, cache, logs).

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// Per-user directories used by the desktop app.
///
/// Built from `ProjectDirs::from("es", "Digio", "Bitacora")`. Index databases live
/// under `<data_dir>/graphs/<graph-hash>/` (ADR-005); see [`AppDirs::graph_data_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// App settings (`settings.json`).
    pub config_dir: PathBuf,
    /// Persistent app data (workspace layout, index databases).
    pub data_dir: PathBuf,
    /// Disposable cache.
    pub cache_dir: PathBuf,
    /// Rolling log files (inside the cache dir).
    pub log_dir: PathBuf,
}

/// Errors resolving or creating the app directories.
#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    /// The OS did not provide a home directory.
    #[error("cannot determine the platform directories (no home directory)")]
    NoHome,
    /// Creating a directory failed.
    #[error("cannot create directory {path}: {source}")]
    Create {
        /// The directory that could not be created.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
}

impl AppDirs {
    /// Resolves the directories for the current user.
    pub fn from_project_dirs() -> Result<Self, PathsError> {
        let dirs = ProjectDirs::from("es", "Digio", "Bitacora").ok_or(PathsError::NoHome)?;
        let cache_dir = dirs.cache_dir().to_path_buf();
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            log_dir: cache_dir.join("logs"),
            cache_dir,
        })
    }

    /// Lays the directories out under one root (tests, portable mode).
    pub fn with_root(root: &Path) -> Self {
        let cache_dir = root.join("cache");
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            log_dir: cache_dir.join("logs"),
            cache_dir,
        }
    }

    /// Creates every directory (idempotent).
    pub fn ensure(&self) -> Result<(), PathsError> {
        for path in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.log_dir,
        ] {
            std::fs::create_dir_all(path).map_err(|source| PathsError::Create {
                path: path.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// `<data_dir>/graphs/<graph-hash>`: where the per-graph index lives (ADR-005).
    pub fn graph_data_dir(&self, graph_root: &Path) -> PathBuf {
        self.data_dir.join("graphs").join(graph_hash(graph_root))
    }

    /// `<config_dir>/settings.json`.
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// `<config_dir>/keymap.json`: key bindings layered over the defaults (BIT-T-0163).
    pub fn keymap_file(&self) -> PathBuf {
        self.config_dir.join("keymap.json")
    }

    /// `<config_dir>/recent-graphs.json`.
    pub fn recent_graphs_file(&self) -> PathBuf {
        self.config_dir.join("recent-graphs.json")
    }

    /// `<data_dir>/workspace.json`.
    pub fn workspace_file(&self) -> PathBuf {
        self.data_dir.join("workspace.json")
    }
}

/// Stable 64-bit FNV-1a hash of the graph path, as 16 hex digits.
///
/// `std`'s `DefaultHasher` is not stable across Rust releases, and this value names
/// an on-disk directory, so the algorithm is spelled out here.
pub fn graph_hash(graph_root: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in graph_root.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_dirs_are_resolved_and_logs_live_in_cache() {
        let dirs = AppDirs::from_project_dirs().expect("home directory available");
        assert!(dirs.log_dir.starts_with(&dirs.cache_dir));
        assert!(
            dirs.config_dir
                .to_string_lossy()
                .to_lowercase()
                .contains("bitacora")
        );
    }

    #[test]
    fn ensure_creates_all_directories_idempotently() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dirs = AppDirs::with_root(tmp.path());
        dirs.ensure().expect("first ensure");
        dirs.ensure().expect("second ensure");
        for dir in [
            &dirs.config_dir,
            &dirs.data_dir,
            &dirs.cache_dir,
            &dirs.log_dir,
        ] {
            assert!(dir.is_dir(), "{} missing", dir.display());
        }
    }

    #[test]
    fn graph_hash_is_stable_and_distinguishes_paths() {
        assert_eq!(graph_hash(Path::new("/a/b")), graph_hash(Path::new("/a/b")));
        assert_ne!(graph_hash(Path::new("/a/b")), graph_hash(Path::new("/a/c")));
        // Pinned value: changing the algorithm would orphan existing indexes.
        assert_eq!(graph_hash(Path::new("")), "cbf29ce484222325");
        let dirs = AppDirs::with_root(Path::new("/r"));
        assert!(
            dirs.graph_data_dir(Path::new("/g"))
                .starts_with("/r/data/graphs")
        );
    }
}
