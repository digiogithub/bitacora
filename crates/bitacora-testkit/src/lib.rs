//! `bitacora-testkit`: dev-only test helpers (fixture loader, temporary graphs and repos).
//!
//! Must only be used from `[dev-dependencies]`; it is never a runtime dependency
//! (`cargo xtask check-deps` rejects any normal-dependency edge into this crate).
//!
//! Fixture graphs live in `fixtures/graphs/<name>/`; use [`graph`] and
//! [`markdown_files`] instead of hard-coding paths.

pub mod git;
mod tracing_setup;

pub use tracing_setup::init_tracing;

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Root directory of the sample Logseq graphs used by tests (`fixtures/graphs`).
pub fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs")
}

/// Kept for compatibility with the skeleton helper; same as [`fixtures_root`].
pub fn fixtures_dir() -> PathBuf {
    fixtures_root()
}

/// Directory of the fixture graph called `name` (for example `"edge-cases"`).
///
/// # Panics
/// Panics when the graph does not exist; this is a test helper, so a typo should fail loudly.
pub fn graph(name: &str) -> PathBuf {
    let dir = fixtures_root().join(name);
    assert!(
        dir.is_dir(),
        "fixture graph `{name}` not found at {}",
        dir.display()
    );
    dir
}

/// Names of all fixture graphs, sorted.
pub fn graph_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures_root())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Sorted Markdown page files (`*.md` under `pages/` and `journals/`) of a fixture graph.
///
/// `PROVENANCE.md`, `LICENSE.md`, `logseq/bak` and `logseq/.recycle` are not pages and are not listed.
pub fn markdown_files(name: &str) -> impl Iterator<Item = PathBuf> {
    let root = graph(name);
    let mut files: Vec<PathBuf> = ["pages", "journals"]
        .iter()
        .flat_map(|sub| WalkDir::new(root.join(sub)).into_iter().flatten())
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    files.into_iter()
}

/// Path of `file` relative to the graph directory, with `/` separators (stable across OSes).
pub fn relative_to_graph(name: &str, file: &Path) -> String {
    let root = graph(name);
    file.strip_prefix(&root)
        .unwrap_or(file)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Create an empty temporary directory that is removed on drop.
pub fn temp_dir() -> std::io::Result<tempfile::TempDir> {
    tempfile::tempdir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dir_is_created() {
        let dir = temp_dir().expect("temp dir");
        assert!(dir.path().is_dir());
        assert!(fixtures_dir().ends_with("fixtures/graphs"));
    }

    #[test]
    fn lists_both_fixture_graphs() {
        let names = graph_names();
        for expected in ["edge-cases", "logseq-docs"] {
            assert!(names.iter().any(|n| n == expected), "{names:?}");
        }
    }

    #[test]
    fn markdown_files_are_sorted_pages_and_journals() {
        let docs: Vec<_> = markdown_files("logseq-docs").collect();
        assert!(docs.len() > 100, "only {} pages", docs.len());
        let mut sorted = docs.clone();
        sorted.sort();
        assert_eq!(docs, sorted);
        assert!(
            docs.iter()
                .all(|p| p.extension().is_some_and(|e| e == "md"))
        );

        let edge: Vec<String> = markdown_files("edge-cases")
            .map(|p| relative_to_graph("edge-cases", &p))
            .collect();
        assert!(edge.iter().any(|p| p == "pages/crlf.md"), "{edge:?}");
        assert!(edge.iter().any(|p| p == "journals/2024_05_01.md"));
        assert!(
            !edge
                .iter()
                .any(|p| p.contains("bak") || p.contains(".recycle"))
        );
    }

    #[test]
    fn byte_exact_fixtures_survived_checkout() {
        let crlf = std::fs::read(graph("edge-cases").join("pages/crlf.md")).expect("crlf.md");
        assert!(crlf.windows(2).any(|w| w == b"\r\n"));
        let bom = std::fs::read(graph("edge-cases").join("pages/bom-utf8.md")).expect("bom");
        assert!(bom.starts_with(&[0xEF, 0xBB, 0xBF]));
    }
}
