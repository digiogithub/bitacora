//! `bitacora-cli serve`: open a runtime session (index, watcher, writer, optional sync) and serve
//! the MCP endpoint (ADR-024). External changes reach the index through the runtime's file
//! watcher; there is no polling reconcile any more.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use bitacora_mcp::McpConfig;
use bitacora_runtime::{McpOptions, RuntimeConfig, Session, SyncOptions};
use clap::Args;

/// Time budget of the ordered shutdown.
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(15);

/// Options for `serve`.
#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Graph folder to serve.
    #[arg(long)]
    pub graph: PathBuf,
    /// TCP port on 127.0.0.1.
    #[arg(long, default_value_t = bitacora_mcp::DEFAULT_PORT)]
    pub port: u16,
    /// Token file (default: `mcp-tokens.json` in the platform config dir).
    #[arg(long)]
    pub token_file: Option<PathBuf>,
    /// Directory holding the index (default: the platform data directory).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Extra browser origin to allow (repeatable).
    #[arg(long = "allow-origin")]
    pub allowed_origins: Vec<String>,
    /// Run the background git sync engine (auto-commit, periodic fetch/push).
    #[arg(long)]
    pub sync: bool,
    /// Branch to sync (with `--sync`).
    #[arg(long, default_value = "main")]
    pub branch: String,
    /// Device name recorded in commit trailers (default: the host name).
    #[arg(long)]
    pub device: Option<String>,
}

/// A running `serve`.
pub struct Running {
    session: Session,
    /// Token file in use.
    pub token_path: PathBuf,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("endpoint", &self.endpoint())
            .finish_non_exhaustive()
    }
}

impl Running {
    /// `http://127.0.0.1:<port>/mcp`.
    pub fn endpoint(&self) -> String {
        self.session.mcp_endpoint().unwrap_or_default()
    }

    /// Ordered shutdown (sync, flush, watcher, index, MCP).
    pub fn stop(self) {
        let report = self.session.shutdown(SHUTDOWN_BUDGET);
        if !report.is_clean() {
            eprintln!("warning: unclean shutdown: {report:?}");
        }
    }
}

/// Open the session and start the MCP server.
pub fn start(args: ServeArgs) -> anyhow::Result<Running> {
    let token_path = args
        .token_file
        .or_else(bitacora_mcp::default_token_path)
        .context("cannot determine the config directory; pass --token-file")?;
    let mut cfg = RuntimeConfig::new(&args.graph);
    cfg.data_dir = args.data_dir;
    cfg.mcp = Some(McpOptions {
        config: McpConfig {
            port: args.port,
            allowed_origins: args.allowed_origins,
            ..McpConfig::default()
        },
        token_path: token_path.clone(),
    });
    if args.sync {
        let device = args
            .device
            .unwrap_or_else(bitacora_sync::repo_setup::hostname);
        cfg.sync = Some(SyncOptions::new(device, args.branch));
    }
    eprintln!("Indexing {} ...", args.graph.display());
    let session = Session::open(cfg).context("opening the graph session")?;
    if let Some(stats) = session.open_stats() {
        for (path, why) in &stats.errors {
            eprintln!("warning: cannot index {path}: {why}");
        }
    }
    Ok(Running {
        session,
        token_path,
    })
}

/// Run until Ctrl-C.
pub fn run(args: ServeArgs) -> anyhow::Result<()> {
    let running = start(args)?;
    eprintln!("MCP endpoint: {}", running.endpoint());
    eprintln!("Token file:   {}", running.token_path.display());
    let waiter = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    waiter.block_on(tokio::signal::ctrl_c())?;
    running.stop();
    Ok(())
}
