//! Index maintenance commands: `reindex` and `doctor`.

pub mod doctor;
pub mod reindex;
pub mod serve;
#[cfg(test)]
mod serve_tests;

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use bitacora_config::EffectiveConfig;
use bitacora_index::IndexLocation;
use clap::Args;

/// Graph and index location options shared by `reindex` and `doctor`.
#[derive(Debug, Args)]
pub struct GraphArgs {
    /// Graph folder.
    #[arg(long)]
    pub graph: PathBuf,
    /// Directory holding the index (default: the platform data directory).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    pub json: bool,
}

/// Resolved graph root and index location.
pub struct Resolved {
    /// Canonical graph root.
    pub graph: PathBuf,
    /// Where the index database lives.
    pub location: IndexLocation,
}

impl GraphArgs {
    /// Canonicalise the graph folder and locate its index.
    pub fn resolve(&self) -> anyhow::Result<Resolved> {
        let graph = self
            .graph
            .canonicalize()
            .with_context(|| format!("graph folder {}", self.graph.display()))?;
        let location = match &self.data_dir {
            Some(dir) => IndexLocation::in_data_dir(dir, &graph),
            None => IndexLocation::for_graph(&graph),
        }
        .context("cannot locate the index")?;
        Ok(Resolved { graph, location })
    }
}

/// Effective config of a graph (global config merged with `logseq/config.edn`).
pub fn load_config(graph: &Path) -> EffectiveConfig {
    EffectiveConfig::load(graph, bitacora_config::global_config_path().as_deref())
}
