//! `bitacora-cli`: headless binary (`serve`, `reindex`, `sync`, `doctor`).

mod cmd;

use std::process::ExitCode;

use bitacora_core as _;
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
    Serve(cmd::serve::ServeArgs),
    /// Rebuild the SQLite index from the graph.
    Reindex(cmd::reindex::ReindexArgs),
    /// Run one git sync cycle.
    Sync,
    /// Diagnose the environment and the graph.
    Doctor(cmd::doctor::DoctorArgs),
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Serve(_) => "serve",
            Command::Reindex(_) => "reindex",
            Command::Sync => "sync",
            Command::Doctor(_) => "doctor",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => match cmd::serve::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("bitacora-cli serve: {e:#}");
                ExitCode::FAILURE
            }
        },
        Command::Reindex(args) => {
            match cmd::reindex::run(&args).and_then(|s| cmd::reindex::print(&s, args.graph.json)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("bitacora-cli reindex: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Doctor(args) => match cmd::doctor::run(&args) {
            Ok(report) => match cmd::doctor::print(&report, args.graph.json) {
                Ok(()) if report.healthy() => ExitCode::SUCCESS,
                Ok(()) => ExitCode::from(cmd::doctor::EXIT_UNHEALTHY),
                Err(e) => {
                    eprintln!("bitacora-cli doctor: {e:#}");
                    ExitCode::FAILURE
                }
            },
            Err(e) => {
                eprintln!("bitacora-cli doctor: {e:#}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("bitacora-cli {}: not implemented", other.name());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subcommands() {
        let cli = Cli::try_parse_from(["bitacora-cli", "sync"]).expect("parse");
        assert_eq!(cli.command.name(), "sync");
        for name in ["reindex", "doctor"] {
            let cli = Cli::try_parse_from(["bitacora-cli", name, "--graph", "/tmp/g", "--json"])
                .expect("parse");
            assert_eq!(cli.command.name(), name);
            assert!(Cli::try_parse_from(["bitacora-cli", name]).is_err());
        }
        let cli =
            Cli::try_parse_from(["bitacora-cli", "serve", "--graph", "/tmp/g", "--port", "0"])
                .expect("parse serve");
        assert_eq!(cli.command.name(), "serve");
        assert!(Cli::try_parse_from(["bitacora-cli", "serve"]).is_err());
    }
}
