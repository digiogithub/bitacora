use std::path::{Component, Path, PathBuf};

use crate::Error;

/// Stable identifier of a graph: `hex(blake3(canonical absolute graph path))[..16]` (design §1.1).
///
/// The path is canonicalized up to its nearest existing ancestor; the rest is made absolute lexically.
pub fn graph_id(graph_root: &Path) -> Result<String, Error> {
    let abs = absolute(graph_root)?;
    let bytes = abs.to_string_lossy();
    let hash = blake3::hash(bytes.as_bytes());
    Ok(hash.to_hex()[..16].to_owned())
}

/// Canonical absolute form of `path`, tolerant of a not-yet-existing tail.
///
/// Walks the path component by component: while the prefix exists it is canonicalized (resolving
/// symlinks such as macOS `/var` -> `/private/var`, Windows 8.3 short names, and `..` after a
/// symlink exactly as the OS would); once it stops existing the rest is applied lexically. The
/// result is therefore comparable with another canonicalized path.
fn absolute(path: &Path) -> Result<PathBuf, Error> {
    let abs = std::path::absolute(path).map_err(|source| Error::GraphRoot {
        path: path.to_owned(),
        source,
    })?;
    let mut cur = PathBuf::new();
    let mut missing = false;
    for comp in abs.components() {
        match comp {
            Component::ParentDir => {
                cur.pop();
            }
            Component::CurDir => {}
            other => cur.push(other.as_os_str()),
        }
        if !missing {
            match cur.canonicalize() {
                Ok(canon) => cur = canon,
                Err(_) => missing = true,
            }
        }
    }
    Ok(cur)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn inside_graph_is_detected_through_symlinked_parents() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("real");
        std::fs::create_dir_all(real.join("graph")).expect("graph");
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        let via_link = link.join("graph");
        let via_real = real.join("graph");
        for (graph, data) in [
            (&via_link, via_real.join(".data")),
            (&via_real, via_link.join(".data")),
            (&via_link, via_link.join(".data")),
        ] {
            let err = IndexLocation::in_data_dir(&data, graph).unwrap_err();
            assert!(
                matches!(err, Error::InsideGraph(_)),
                "{data:?} in {graph:?}"
            );
        }
        // Outside stays allowed and ids agree across spellings.
        let ok = IndexLocation::in_data_dir(&real.join("data"), &via_link).expect("outside");
        assert_eq!(ok.graph_id(), graph_id(&via_real).expect("id"));
    }

    #[test]
    fn missing_tail_with_dotdot_is_normalized_lexically() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let graph = tmp.path().join("g");
        std::fs::create_dir_all(&graph).expect("graph");
        let inside = graph.join("x").join("..").join("d");
        assert!(matches!(
            IndexLocation::in_data_dir(&inside, &graph),
            Err(Error::InsideGraph(_))
        ));
        let outside = graph.join("x").join("..").join("..").join("d");
        assert!(IndexLocation::in_data_dir(&outside, &graph).is_ok());
    }
}
