//! Command-line arguments of the desktop binary.

use std::path::PathBuf;

use clap::Parser;

/// Bitacora desktop outliner.
#[derive(Debug, Clone, Parser, PartialEq, Eq)]
#[command(name = "bitacora", version, about)]
pub struct Args {
    /// Path of the Logseq graph folder to open.
    #[arg(long, value_name = "PATH")]
    pub graph: Option<PathBuf>,

    /// Default log filter (overridden by `RUST_LOG`), e.g. `debug` or `info,bitacora_app=trace`.
    #[arg(long, value_name = "LEVEL")]
    pub log_level: Option<String>,

    /// Open the window, wait for the first frame, log its timing and exit with 0.
    #[arg(long)]
    pub smoke_test: bool,

    /// Open the block editor spike (ADR-002 / BIT-US-0040) instead of the workspace.
    #[arg(long)]
    pub spike_editor: bool,

    /// Number of generated blocks in the spike page.
    #[arg(long, value_name = "N", default_value_t = 1000)]
    pub spike_blocks: usize,

    /// Load this Logseq page into the spike instead of generated content.
    #[arg(long, value_name = "FILE")]
    pub spike_page: Option<PathBuf>,

    /// Run the spike benchmark, print `SPIKE_BENCH {json}` and exit.
    #[arg(long)]
    pub spike_bench: bool,
}

impl Args {
    /// Window title: "Bitacora — <graph name>" or just "Bitacora".
    pub fn window_title(&self) -> String {
        match self.graph_name() {
            Some(name) => format!("Bitacora \u{2014} {name}"),
            None => "Bitacora".to_owned(),
        }
    }

    /// Last path component of the graph folder.
    pub fn graph_name(&self) -> Option<String> {
        let path = self.graph.as_ref()?;
        let path = path.canonicalize().unwrap_or_else(|_| path.clone());
        path.file_name().map(|n| n.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_graph_and_flags() {
        let args = Args::parse_from(["bitacora", "--graph", "/tmp/my-graph", "--smoke-test"]);
        assert_eq!(args.graph, Some(PathBuf::from("/tmp/my-graph")));
        assert!(args.smoke_test);
        assert_eq!(args.window_title(), "Bitacora \u{2014} my-graph");
    }

    #[test]
    fn title_without_graph() {
        let args = Args::parse_from(["bitacora"]);
        assert_eq!(args.window_title(), "Bitacora");
        assert!(args.log_level.is_none());
    }
}
