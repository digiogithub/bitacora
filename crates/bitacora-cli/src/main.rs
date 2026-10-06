//! `bitacora-cli`: headless binary (`serve`, `reindex`, `sync`, `doctor`).

mod cmd;

use std::process::ExitCode;

use bitacora_core as _;
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
    Sync(cmd::sync::SyncArgs),
    /// Diagnose the environment and the graph.
    Doctor(cmd::doctor::DoctorArgs),
}

impl Command {
    #[cfg(test)]
    fn name(&self) -> &'static str {
        match self {
            Command::Serve(_) => "serve",
            Command::Reindex(_) => "reindex",
            Command::Sync(_) => "sync",
            Command::Doctor(_) => "doctor",
        }
    }
}

/// Installs the crash-report panic hook (BIT-US-0111); the report path is printed on panic.
fn install_crash_hook() {
    use bitacora_runtime::crash::{CrashConfig, GraphRoots, install_panic_hook};
    let Some(data_dir) = bitacora_runtime::instance::default_dir() else {
        return;
    };
    install_panic_hook(CrashConfig {
        dir: data_dir.join("crashes"),
        log_dir: None,
        binary: "bitacora-cli".to_owned(),
        graphs: GraphRoots::default(),
    });
}

fn main() -> ExitCode {
    install_crash_hook();
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => match cmd::serve::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("bitacora-cli serve: {e:#}");
                ExitCode::FAILURE
            }
        },
        Command::Sync(args) => match cmd::sync::run(&args) {
            Ok(o) => {
                cmd::sync::print(&o);
                if o.ok() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("bitacora-cli sync: {e:#}");
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subcommands() {
        let cli =
            Cli::try_parse_from(["bitacora-cli", "sync", "--graph", "/tmp/g"]).expect("parse");
        assert_eq!(cli.command.name(), "sync");
        assert!(Cli::try_parse_from(["bitacora-cli", "sync"]).is_err());
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

    #[test]
    fn serve_permission_flags_default_off() {
        let parse = |extra: &[&str]| {
            let mut a = vec!["bitacora-cli", "serve", "--graph", "/tmp/g"];
            a.extend_from_slice(extra);
            match Cli::try_parse_from(a).expect("parse serve").command {
                Command::Serve(s) => s,
                other => panic!("not serve: {}", other.name()),
            }
        };
        let off = parse(&[]);
        assert!(!off.allow_writes && !off.allow_deletes && !off.api);
        let on = parse(&["--allow-writes", "--allow-deletes", "--api"]);
        assert!(on.allow_writes && on.allow_deletes && on.api);
    }

    #[test]
    fn serve_help_documents_permission_flags() {
        use clap::CommandFactory as _;
        let mut cmd = Cli::command();
        let serve = cmd.find_subcommand_mut("serve").expect("serve");
        let help = serve.render_long_help().to_string();
        for flag in ["--allow-writes", "--allow-deletes", "--api"] {
            assert!(help.contains(flag), "{help}");
        }
    }
}
