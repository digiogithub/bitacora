//! `bitacora-cli`: headless binary (`serve`, `reindex`, `sync`, `doctor`).

mod cmd;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context as _;
use bitacora_core as _;
use bitacora_mcp::{McpConfig, McpServer, StaticGraphReader, TokenStore};
use bitacora_sync as _;
use clap::{Args, Parser, Subcommand};

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
    Serve(ServeArgs),
    /// Rebuild the SQLite index from the graph.
    Reindex(cmd::reindex::ReindexArgs),
    /// Run one git sync cycle.
    Sync,
    /// Diagnose the environment and the graph.
    Doctor(cmd::doctor::DoctorArgs),
}

/// Options for `serve`.
#[derive(Debug, Args)]
struct ServeArgs {
    /// Graph folder to serve.
    #[arg(long)]
    graph: PathBuf,
    /// TCP port on 127.0.0.1.
    #[arg(long, default_value_t = bitacora_mcp::DEFAULT_PORT)]
    port: u16,
    /// Token file (default: `mcp-tokens.json` in the platform config dir).
    #[arg(long)]
    token_file: Option<PathBuf>,
    /// Extra browser origin to allow (repeatable).
    #[arg(long = "allow-origin")]
    allowed_origins: Vec<String>,
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

fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let graph = args
        .graph
        .canonicalize()
        .with_context(|| format!("graph folder {}", args.graph.display()))?;
    let token_path = args
        .token_file
        .or_else(bitacora_mcp::default_token_path)
        .context("cannot determine the config directory; pass --token-file")?;
    let tokens = Arc::new(TokenStore::load_or_init(&token_path)?);
    let name = graph
        .file_name()
        .map_or_else(|| "graph".to_owned(), |n| n.to_string_lossy().into_owned());
    let reader = Arc::new(StaticGraphReader::new(name, graph.display().to_string()));
    let config = McpConfig {
        port: args.port,
        allowed_origins: args.allowed_origins,
        ..McpConfig::default()
    };
    let server = McpServer::start(config, reader, tokens)?;
    eprintln!("MCP endpoint: {}", server.endpoint());
    eprintln!("Token file:   {}", token_path.display());
    let waiter = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    waiter.block_on(tokio::signal::ctrl_c())?;
    server.stop();
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => match serve(args) {
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
