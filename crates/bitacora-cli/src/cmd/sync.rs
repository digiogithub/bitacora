//! `bitacora-cli sync`: one git sync cycle through the runtime (commit, fetch, merge, push).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use bitacora_runtime::{RuntimeConfig, Session, SyncOptions};
use bitacora_sync::state::SyncState;
use clap::Args;

/// Options of `sync`.
#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Graph folder (must be a git repository with a remote).
    #[arg(long)]
    pub graph: PathBuf,
    /// Directory holding the index (default: the platform data directory).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Branch to sync.
    #[arg(long, default_value = "main")]
    pub branch: String,
    /// Device name recorded in commit trailers (default: the host name).
    #[arg(long)]
    pub device: Option<String>,
}

/// Outcome of a one-shot sync.
#[derive(Debug)]
pub struct Outcome {
    /// Final state of the cycle.
    pub state: SyncState,
    /// Local commits not pushed.
    pub ahead: usize,
    /// Unresolved conflicts.
    pub conflicts: usize,
    /// Last error, if any.
    pub error: Option<String>,
}

impl Outcome {
    /// The graph is in sync.
    pub fn ok(&self) -> bool {
        self.state == SyncState::Idle
    }
}

/// Run one cycle. No watcher or MCP server is started; the index follows the files the merge
/// writes through the runtime.
pub fn run(args: &SyncArgs) -> anyhow::Result<Outcome> {
    let mut cfg = RuntimeConfig::new(&args.graph);
    cfg.data_dir.clone_from(&args.data_dir);
    cfg.watch = None;
    let device = args
        .device
        .clone()
        .unwrap_or_else(bitacora_sync::repo_setup::hostname);
    let mut opts = SyncOptions::new(device, args.branch.clone());
    opts.background = false;
    cfg.sync = Some(opts);
    let session = Session::open(cfg).context("opening the graph session")?;
    let result = session.sync_once().context("sync");
    let report = session.shutdown(Duration::from_secs(30));
    if !report.is_clean() {
        eprintln!("warning: unclean shutdown: {report:?}");
    }
    let (state, status) = result?;
    Ok(Outcome {
        state,
        ahead: status.ahead,
        conflicts: status.conflicts,
        error: status.last_error,
    })
}

/// Print the outcome.
pub fn print(o: &Outcome) {
    println!("State:     {:?}", o.state);
    println!("Unpushed:  {}", o.ahead);
    println!("Conflicts: {}", o.conflicts);
    if let Some(e) = &o.error {
        println!("Error:     {e}");
    }
}
