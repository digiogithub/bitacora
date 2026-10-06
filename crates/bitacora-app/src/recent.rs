//! Recently opened graphs, persisted as JSON in the app config directory.
//!
//! Losing this file is harmless (it is only a convenience list), so a corrupt or missing
//! file loads as empty and writes are best-effort atomic (temp file, fsync, rename).

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Most graphs kept.
pub const MAX_RECENT: usize = 10;

/// One recent graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentGraph {
    /// Graph folder.
    pub path: PathBuf,
}

impl RecentGraph {
    /// Folder name shown as the graph name.
    pub fn name(&self) -> String {
        graph_name(&self.path)
    }

    /// Whether the folder still exists.
    pub fn exists(&self) -> bool {
        self.path.is_dir()
    }
}

/// The folder name of a graph path.
pub fn graph_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Whether `root` looks like a Logseq graph (`logseq/config.edn` exists).
pub fn has_graph_config(root: &Path) -> bool {
    root.join("logseq").join("config.edn").is_file()
}

/// Most recent first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecentGraphs {
    graphs: Vec<RecentGraph>,
}

impl RecentGraphs {
    /// Loads the list; a missing or unreadable file gives an empty list.
    pub fn load(file: &Path) -> Self {
        std::fs::read(file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the list atomically.
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        let dir = file.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, file)
    }

    /// The graphs, most recent first.
    pub fn graphs(&self) -> &[RecentGraph] {
        &self.graphs
    }

    /// Moves (or adds) `path` to the front, keeping at most [`MAX_RECENT`].
    pub fn touch(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.graphs.retain(|g| g.path != path);
        self.graphs.insert(0, RecentGraph { path });
        self.graphs.truncate(MAX_RECENT);
    }

    /// Forgets `path`.
    pub fn remove(&mut self, path: &Path) {
        self.graphs.retain(|g| g.path != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_orders_dedupes_and_caps() {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut recent = RecentGraphs::default();
        let dirs: Vec<_> = (0..12)
            .map(|i| {
                let d = tmp.path().join(format!("g{i}"));
                std::fs::create_dir(&d).expect("mkdir");
                d
            })
            .collect();
        for d in &dirs {
            recent.touch(d);
        }
        assert_eq!(recent.graphs().len(), MAX_RECENT);
        recent.touch(&dirs[5]);
        assert_eq!(recent.graphs()[0].name(), "g5");
        assert_eq!(recent.graphs().len(), MAX_RECENT);
        let before = recent.graphs().len();
        recent.touch(&dirs[5]);
        assert_eq!(recent.graphs().len(), before);
        recent.remove(&recent.graphs()[0].path.clone());
        assert_eq!(recent.graphs().len(), before - 1);
    }

    #[test]
    fn round_trips_and_tolerates_corruption() {
        let tmp = tempfile::tempdir().expect("tmp");
        let file = tmp.path().join("cfg").join("recent.json");
        let mut recent = RecentGraphs::default();
        recent.touch(tmp.path());
        recent.save(&file).expect("save");
        assert_eq!(RecentGraphs::load(&file), recent);
        std::fs::write(&file, b"{not json").expect("write");
        assert!(RecentGraphs::load(&file).graphs().is_empty());
        assert!(
            RecentGraphs::load(&tmp.path().join("missing.json"))
                .graphs()
                .is_empty()
        );
    }

    #[test]
    fn detects_logseq_config() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert!(!has_graph_config(tmp.path()));
        std::fs::create_dir(tmp.path().join("logseq")).expect("mkdir");
        std::fs::write(tmp.path().join("logseq/config.edn"), "{}").expect("write");
        assert!(has_graph_config(tmp.path()));
    }
}
