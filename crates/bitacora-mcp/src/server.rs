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
use crate::guard::{GuardState, guard};
use crate::handler::BitacoraMcp;
use crate::reader::GraphReader;
use crate::tokens::TokenStore;

/// Default `mcp.port` (Logseq's API uses 12315).
pub const DEFAULT_PORT: u16 = 12316;

/// Server settings (`mcp.*`, design `mcp-server.md` section 8).
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// Bind address; must be loopback (`127.0.0.1` or `::1`).
    pub bind: IpAddr,
    /// TCP port; `0` picks an ephemeral port (tests). A busy port is an error, never a fallback.
    pub port: u16,
    /// Extra browser origins allowed (`mcp.allowed_origins`). Empty by default.
    pub allowed_origins: Vec<String>,
    /// `true` keeps stateful sessions (`LocalSessionManager`); `false` serves stateless JSON responses.
    pub stateful: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: DEFAULT_PORT,
            allowed_origins: Vec::new(),
            stateful: true,
        }
    }
}

/// A running MCP server. Dropping it (or calling [`stop`](Self::stop)) shuts it down.
pub struct McpServer {
    runtime: Option<tokio::runtime::Runtime>,
    addr: SocketAddr,
    shutdown: watch::Sender<bool>,
    tokens: Arc<TokenStore>,
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
        let mcp = StreamableHttpService::new(
            move || Ok(BitacoraMcp::new(Arc::clone(&reader))),
            Arc::new(LocalSessionManager::default()),
            http_config,
        );

        let state = GuardState {
            port,
            allowed_origins: Arc::new(config.allowed_origins),
            tokens: Arc::clone(&tokens),
        };
        let router = Router::new()
            .route(
                "/health",
                get(|| async { axum::Json(serde_json::json!({ "ok": true })) }),
            )
            .nest_service("/mcp", mcp)
            .layer(from_fn_with_state(state, guard));

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
        })
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
