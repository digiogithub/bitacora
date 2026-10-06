use std::path::{Path, PathBuf};

use crate::Error;

/// Stable identifier of a graph: `hex(blake3(canonical absolute graph path))[..16]` (design §1.1).
///
/// The path is canonicalized when it exists; otherwise it is made absolute lexically.
pub fn graph_id(graph_root: &Path) -> Result<String, Error> {
    let abs = absolute(graph_root)?;
    let bytes = abs.to_string_lossy();
    let hash = blake3::hash(bytes.as_bytes());
    Ok(hash.to_hex()[..16].to_owned())
}

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    match path.canonicalize() {
        Ok(p) => Ok(p),
        Err(_) => std::path::absolute(path).map_err(|source| Error::GraphRoot {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Where a graph's index database lives: `<data_dir>/bitacora/graphs/<graph-id>/index.sqlite` (ADR-005).
///
/// The index is always outside the graph folder so git, sync tools and Logseq never see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexLocation {
    graph_id: String,
    dir: PathBuf,
}

impl IndexLocation {
    /// File name of the database inside the per-graph directory.
    pub const FILE_NAME: &'static str = "index.sqlite";

    /// Location under the platform data directory (`directories::BaseDirs::data_dir`).
    pub fn for_graph(graph_root: &Path) -> Result<Self, Error> {
        let base = directories::BaseDirs::new().ok_or(Error::NoDataDir)?;
        Self::in_data_dir(base.data_dir(), graph_root)
    }

    /// Location under an explicit data directory (tests, portable mode, `--data-dir`).
    ///
    /// Fails with [`Error::InsideGraph`] if the resulting directory is inside the graph folder.
    pub fn in_data_dir(data_dir: &Path, graph_root: &Path) -> Result<Self, Error> {
        let id = graph_id(graph_root)?;
        let dir = data_dir.join("bitacora").join("graphs").join(&id);
        let root = absolute(graph_root)?;
        if absolute(&dir)?.starts_with(&root) {
            return Err(Error::InsideGraph(dir));
        }
        Ok(Self { graph_id: id, dir })
    }

    /// The 16-hex-char graph id.
    pub fn graph_id(&self) -> &str {
        &self.graph_id
    }

    /// The per-graph directory (holds the database and its WAL files).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Full path of the SQLite database file.
    pub fn db_path(&self) -> PathBuf {
        self.dir.join(Self::FILE_NAME)
    }
}
