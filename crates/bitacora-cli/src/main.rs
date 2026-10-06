//! `bitacora-cli`: headless binary (`serve`, `reindex`, `sync`, `doctor`).

use std::process::ExitCode;

use bitacora_core as _;
use bitacora_index as _;
use bitacora_mcp as _;
use bitacora_sync as _;
use clap::{Parser, Subcommand};

/// Headless Bitacora commands.
#[derive(Debug, Parser)]
#[command(name = "bitacora-cli", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the MCP endpoint over HTTP.
    Serve,
    /// Rebuild the SQLite index from the graph.
    Reindex,
    /// Run one git sync cycle.
    Sync,
    /// Diagnose the environment and the graph.
    Doctor,
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Serve => "serve",
            Command::Reindex => "reindex",
            Command::Sync => "sync",
            Command::Doctor => "doctor",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    eprintln!("bitacora-cli {}: not implemented", cli.command.name());
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subcommands() {
        for name in ["serve", "reindex", "sync", "doctor"] {
            let cli = Cli::try_parse_from(["bitacora-cli", name]).expect("parse");
            assert_eq!(cli.command.name(), name);
        }
    }
}
