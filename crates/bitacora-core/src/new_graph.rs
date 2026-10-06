//! New graph creation (BIT-US-0099, `docs/analysis/logseq/01-file-graph-layout.md` sections 1
//! and 2.3).
//!
//! Creates the layout Logseq 0.10.x expects, with Bitacora's own default `config.edn`
//! ([`bitacora_config::DEFAULT_CONFIG_EDN`], authored here, not Logseq's template, ADR-015):
//!
//! ```text
//! pages/contents.md        "-"
//! journals/
//! logseq/config.edn        Bitacora default (triple-lowbar file names)
//! logseq/custom.css        empty
//! logseq/.recycle/
//! ```
//!
//! `assets/` is created on first use. A folder that already is a graph is left untouched; any
//! other non-empty folder is refused. This is the one place besides the writer that creates
//! graph files, and it only ever writes into an empty folder (single-writer guard allow-list).

use std::io;
use std::path::{Path, PathBuf};

use bitacora_config::DEFAULT_CONFIG_EDN;
use bitacora_config::config::GRAPH_CONFIG_REL_PATH;

use crate::editor::fsio::atomic_write;
use crate::recycle::RECYCLE_DIR;

/// Entries that do not make a folder "non-empty" (a fresh `git init`).
const IGNORED_ENTRIES: &[&str] = &[".git", ".gitignore"];

/// What [`create_graph`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewGraphOutcome {
    /// The graph skeleton was created.
    Created,
    /// The folder already is a graph (it has `logseq/config.edn`); nothing was changed.
    AlreadyGraph,
}

/// Why a graph could not be created.
#[derive(Debug, thiserror::Error)]
pub enum NewGraphError {
    /// The folder has files that are not a graph.
    #[error("`{}` is not empty and is not a graph", .0.display())]
    NotEmpty(PathBuf),
    /// The path exists and is not a directory.
    #[error("`{}` is not a directory", .0.display())]
    NotADirectory(PathBuf),
    /// File system error.
    #[error("cannot create the graph: {0}")]
    Io(#[from] io::Error),
}

/// Whether `root` already is a graph.
#[must_use]
pub fn is_graph(root: &Path) -> bool {
    root.join(GRAPH_CONFIG_REL_PATH).is_file()
}

/// Creates a new graph in `root` (created when missing, must be empty otherwise).
///
/// # Errors
/// [`NewGraphError::NotEmpty`] for a non-empty folder that is not a graph; I/O errors.
pub fn create_graph(root: &Path) -> Result<NewGraphOutcome, NewGraphError> {
    if is_graph(root) {
        return Ok(NewGraphOutcome::AlreadyGraph);
    }
    if root.exists() {
        if !root.is_dir() {
            return Err(NewGraphError::NotADirectory(root.to_owned()));
        }
        for e in std::fs::read_dir(root)? {
            let name = e?.file_name();
            if !IGNORED_ENTRIES.iter().any(|i| name == *i) {
                return Err(NewGraphError::NotEmpty(root.to_owned()));
            }
        }
    }
    std::fs::create_dir_all(root.join("pages"))?;
    std::fs::create_dir_all(root.join("journals"))?;
    std::fs::create_dir_all(root.join(RECYCLE_DIR))?;
    atomic_write(&root.join("pages/contents.md"), b"-\n")?;
    atomic_write(&root.join("logseq/custom.css"), b"")?;
    // The config goes last: its presence is what marks the folder as a graph.
    atomic_write(
        &root.join(GRAPH_CONFIG_REL_PATH),
        DEFAULT_CONFIG_EDN.as_bytes(),
    )?;
    Ok(NewGraphOutcome::Created)
}
