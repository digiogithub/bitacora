//! `bitacora-cli serve`: index the graph, keep the index fresh and serve the MCP endpoint.
//!
//! The headless process has no file watcher yet, so a background thread re-runs the incremental
//! reconcile every [`RECONCILE_EVERY`]; index events then drive MCP resource notifications.
//! Sync status is `disabled` until the sync engine is wired in (BIT-US-0045+).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Context as _;
use bitacora_index::{Index, Indexer, IndexerOptions, OpenOptions};
use bitacora_mcp::{IndexGraphReader, McpConfig, McpServer, TokenStore};
use clap::Args;

use super::load_config;

/// How often the incremental reconcile runs while serving.
const RECONCILE_EVERY: Duration = Duration::from_secs(2);

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
}

/// A running `serve`: the MCP server plus the index maintenance thread.
pub struct Running {
    server: McpServer,
    stop: Arc<AtomicBool>,
    poller: Option<JoinHandle<Indexer>>,
    /// Token file in use.
    pub token_path: PathBuf,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("endpoint", &self.server.endpoint())
            .finish_non_exhaustive()
    }
}

impl Running {
    /// `http://127.0.0.1:<port>/mcp`.
    pub fn endpoint(&self) -> String {
        self.server.endpoint()
    }

    /// Stop serving and shut the indexer down.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(poller) = self.poller.take()
            && let Ok(indexer) = poller.join()
        {
            indexer.shutdown();
        }
        self.server.stop();
    }
}

/// Build the index, start the MCP server and the reconcile thread.
pub fn start(args: ServeArgs) -> anyhow::Result<Running> {
    let graph = args
        .graph
        .canonicalize()
        .with_context(|| format!("graph folder {}", args.graph.display()))?;
    let token_path = args
        .token_file
        .or_else(bitacora_mcp::default_token_path)
        .context("cannot determine the config directory; pass --token-file")?;
    let tokens = Arc::new(TokenStore::load_or_init(&token_path)?);

    let location = match &args.data_dir {
        Some(dir) => bitacora_index::IndexLocation::in_data_dir(dir, &graph),
        None => bitacora_index::IndexLocation::for_graph(&graph),
    }
    .context("cannot locate the index")?;
    let config = load_config(&graph);
    let index = Index::open(location, OpenOptions::for_config(&graph, &config))
        .context("opening the index")?;
    let indexer = Indexer::start(&index, IndexerOptions::new(&graph, config))
        .context("starting the indexer")?;
    let events = indexer.subscribe();
    eprintln!("Indexing {} ...", graph.display());
    let stats = indexer.reconcile().context("building the index")?;
    for (path, why) in &stats.errors {
        eprintln!("warning: cannot index {path}: {why}");
    }

    let name = graph
        .file_name()
        .map_or_else(|| "graph".to_owned(), |n| n.to_string_lossy().into_owned());
    let reader = IndexGraphReader::new(&index, graph, name);
    reader.forward_events(events);

    let config = McpConfig {
        port: args.port,
        allowed_origins: args.allowed_origins,
        ..McpConfig::default()
    };
    let server = McpServer::start(config, Arc::new(reader), tokens)?;

    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let poller = std::thread::Builder::new()
        .name("bitacora-reconcile".into())
        .spawn(move || {
            // The index must outlive the indexer thread's readers.
            let _index = index;
            while !flag.load(Ordering::SeqCst) {
                std::thread::sleep(RECONCILE_EVERY);
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(e) = indexer.reconcile() {
                    eprintln!("warning: reconcile failed: {e}");
                }
            }
            indexer
        })
        .context("starting the reconcile thread")?;
    Ok(Running {
        server,
        stop,
        poller: Some(poller),
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
