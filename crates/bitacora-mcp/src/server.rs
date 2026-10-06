//! Server lifecycle: dedicated tokio runtime, loopback-only bind, start/stop from synchronous code.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::sync::watch;

use crate::Error;
use crate::audit::{AuditLog, UndoError};
use crate::bridge::QueueBridge;
use crate::compat;
use crate::guard::{GuardState, guard};
use crate::handler::{BitacoraMcp, Services};
use crate::policy::{OpenGate, WriteGate, WritePolicy};
use crate::reader::GraphReader;
use crate::status::{DisabledSync, SyncStatusProvider};
use crate::tokens::TokenStore;

/// Default `mcp.port` (Logseq's API uses 12315).
pub const DEFAULT_PORT: u16 = 12316;

/// Server settings (`mcp.*`, design `mcp-server.md` section 8).
#[derive(Clone)]
pub struct McpConfig {
    /// Bind address; must be loopback (`127.0.0.1` or `::1`).
    pub bind: IpAddr,
    /// TCP port; `0` picks an ephemeral port (tests). A busy port is an error, never a fallback.
    pub port: u16,
    /// Extra browser origins allowed (`mcp.allowed_origins`). Empty by default.
    pub allowed_origins: Vec<String>,
    /// `true` keeps stateful sessions (`LocalSessionManager`); `false` serves stateless JSON responses.
    pub stateful: bool,
    /// `mcp.allow_writes`: agents may create and edit (default off). Needs a write pipeline.
    pub allow_writes: bool,
    /// `mcp.allow_deletes`: agents may remove blocks and delete or rename pages (default off).
    pub allow_deletes: bool,
    /// `mcp.protected_namespaces`: pages under these namespaces refuse agent writes.
    pub protected_namespaces: Vec<String>,
    /// Write operations allowed per token and minute (default 60).
    pub writes_per_minute: usize,
    /// Serve the optional Logseq-compatible `POST /api` endpoint (default off, `404`).
    pub api_enabled: bool,
    /// Directory of the JSONL audit log; `None` keeps the log in memory only.
    pub audit_dir: Option<std::path::PathBuf>,
    /// Reports blocks being edited in the UI (`BLOCK_BUSY`); default: none.
    pub gate: Option<Arc<dyn WriteGate>>,
}

impl std::fmt::Debug for McpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConfig")
            .field("bind", &self.bind)
            .field("port", &self.port)
            .field("allow_writes", &self.allow_writes)
            .field("allow_deletes", &self.allow_deletes)
            .field("api_enabled", &self.api_enabled)
            .finish_non_exhaustive()
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: DEFAULT_PORT,
            allowed_origins: Vec::new(),
            stateful: true,
            allow_writes: false,
            allow_deletes: false,
            protected_namespaces: Vec::new(),
            writes_per_minute: crate::policy::DEFAULT_WRITES_PER_MINUTE,
            api_enabled: false,
            audit_dir: None,
            gate: None,
        }
    }
}

/// Everything a server needs besides its [`McpConfig`].
pub struct ServerParts {
    /// Read access to the graph.
    pub reader: Arc<dyn GraphReader>,
    /// Sync status source (and `git_sync_now` trigger).
    pub sync: Arc<dyn SyncStatusProvider>,
    /// Bearer tokens.
    pub tokens: Arc<TokenStore>,
    /// The write pipeline; without it every write tool answers `READ_ONLY`.
    pub writer: Option<QueueBridge>,
    /// Reports blocks being edited in the UI (`BLOCK_BUSY`); default: none.
    pub gate: Option<Arc<dyn WriteGate>>,
}

impl std::fmt::Debug for ServerParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerParts")
            .field("writer", &self.writer.is_some())
            .finish_non_exhaustive()
    }
}

impl ServerParts {
    /// Read-only parts with no sync and no write pipeline.
    pub fn read_only(reader: Arc<dyn GraphReader>, tokens: Arc<TokenStore>) -> Self {
        Self {
            reader,
            sync: Arc::new(DisabledSync),
            tokens,
            writer: None,
            gate: None,
        }
    }
}

/// A running MCP server. Dropping it (or calling [`stop`](Self::stop)) shuts it down.
pub struct McpServer {
    runtime: Option<tokio::runtime::Runtime>,
    addr: SocketAddr,
    shutdown: watch::Sender<bool>,
    tokens: Arc<TokenStore>,
    policy: Arc<WritePolicy>,
    audit: Arc<AuditLog>,
    writer: Option<Arc<QueueBridge>>,
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl McpServer {
    /// Bind and start serving on a dedicated runtime. Returns once the socket is listening, so a
    /// busy port or non-loopback address is reported synchronously.
    pub fn start(
        config: McpConfig,
        reader: Arc<dyn GraphReader>,
        tokens: Arc<TokenStore>,
    ) -> Result<Self, Error> {
        Self::start_with(config, ServerParts::read_only(reader, tokens))
    }

    /// Like [`start`](Self::start) with a sync status source for `git_sync_status` and
    /// `bitacora://sync/status`.
    pub fn start_with_sync(
        config: McpConfig,
        reader: Arc<dyn GraphReader>,
        sync: Arc<dyn SyncStatusProvider>,
        tokens: Arc<TokenStore>,
    ) -> Result<Self, Error> {
        Self::start_with(
            config,
            ServerParts {
                sync,
                ..ServerParts::read_only(reader, tokens)
            },
        )
    }

    /// Full constructor: reader, sync, tokens, optional write pipeline and editor gate.
    pub fn start_with(config: McpConfig, parts: ServerParts) -> Result<Self, Error> {
        let ServerParts {
            reader,
            sync,
            tokens,
            writer,
            gate,
        } = parts;
        let writer = writer.map(Arc::new);
        let policy = Arc::new(
            WritePolicy::new(
                config.allow_writes,
                config.allow_deletes,
                config.protected_namespaces.clone(),
            )
            .with_rate_limit(config.writes_per_minute, std::time::Duration::from_secs(60)),
        );
        let audit = Arc::new(match &config.audit_dir {
            Some(dir) => AuditLog::open(dir)?,
            None => AuditLog::in_memory(),
        });
        let services = Arc::new(Services {
            reader,
            sync,
            writer: writer.clone(),
            policy: Arc::clone(&policy),
            gate: gate
                .or_else(|| config.gate.clone())
                .unwrap_or_else(|| Arc::new(OpenGate)),
            audit: Arc::clone(&audit),
        });
        if !config.bind.is_loopback() {
            return Err(Error::NonLoopbackBind(config.bind));
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("bitacora-mcp")
            .enable_all()
            .build()?;
        let requested = SocketAddr::new(config.bind, config.port);
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind(requested))
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    Error::PortInUse(config.port)
                } else {
                    Error::Io(e)
                }
            })?;
        let addr = listener.local_addr()?;
        let port = addr.port();

        let mut http_config = StreamableHttpServerConfig::default()
            .with_allowed_hosts([
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("[::1]:{port}"),
            ])
            .with_sse_keep_alive(None);
        http_config = if config.stateful {
            http_config.with_legacy_session_mode(true)
        } else {
            http_config
                .with_legacy_session_mode(false)
                .with_json_response(true)
        };
        let rmcp_cancel = http_config.cancellation_token.clone();
        let api_services = Arc::clone(&services);
        let mcp = StreamableHttpService::new(
            move || Ok(BitacoraMcp::new(Arc::clone(&services))),
            Arc::new(LocalSessionManager::default()),
            http_config,
        );

        let state = GuardState {
            port,
            allowed_origins: Arc::new(config.allowed_origins),
            tokens: Arc::clone(&tokens),
            audit: Arc::clone(&audit),
        };
        let mut router = Router::new()
            .route(
                "/health",
                get(|| async { axum::Json(serde_json::json!({ "ok": true })) }),
            )
            .nest_service("/mcp", mcp);
        if config.api_enabled {
            router = router.merge(compat::router(api_services));
        }
        let router = router.layer(from_fn_with_state(state, guard));

        let (shutdown, mut rx) = watch::channel(false);
        runtime.spawn(async move {
            let graceful = async move {
                let _ = rx.wait_for(|stop| *stop).await;
                rmcp_cancel.cancel();
            };
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(graceful)
                .await
            {
                tracing::error!(error = %e, "MCP server terminated");
            }
        });
        tracing::info!(%addr, "MCP server listening");
        Ok(Self {
            runtime: Some(runtime),
            addr,
            shutdown,
            tokens,
            policy,
            audit,
            writer,
        })
    }

    /// The live write policy (toggles, protected namespaces).
    pub fn policy(&self) -> &Arc<WritePolicy> {
        &self.policy
    }

    /// The audit log (list entries, filter by token or tool).
    pub fn audit(&self) -> &Arc<AuditLog> {
        &self.audit
    }

    /// Undoes one audited write through the command queue (one undo step for the user).
    ///
    /// # Errors
    /// [`UndoError`] when the entry is unknown, not a write, already undone, its undo data is gone
    /// (app restarted) or the blocks changed since.
    pub fn undo_audit_entry(&self, id: &str) -> Result<(), UndoError> {
        let data = self.audit.undo_data(id)?;
        let bridge = self
            .writer
            .as_ref()
            .ok_or_else(|| UndoError::Failed("no write pipeline".into()))?;
        bridge.undo(&data)?;
        self.audit.mark_undone(id);
        Ok(())
    }

    /// The bound socket address (resolves port `0`).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://127.0.0.1:<port>/mcp`.
    pub fn endpoint(&self) -> String {
        match self.addr {
            SocketAddr::V4(a) => format!("http://{a}/mcp"),
            SocketAddr::V6(a) => format!("http://[{}]:{}/mcp", a.ip(), a.port()),
        }
    }

    /// The live token registry (create/revoke/rotate take effect immediately).
    pub fn tokens(&self) -> &Arc<TokenStore> {
        &self.tokens
    }

    /// Stop accepting connections and shut the runtime down.
    pub fn stop(mut self) {
        self.shutdown_inner();
    }

    fn shutdown_inner(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_timeout(std::time::Duration::from_secs(2));
        }
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}
