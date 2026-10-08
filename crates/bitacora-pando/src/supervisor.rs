//! Seam for managed mode (ADR-029): who runs `pando serve` for a graph.
//!
//! The service only needs "give me a loopback endpoint (and tokens) for this graph" and "stop it".
//! The real implementation is [`crate::managed::ManagedSupervisor`] (per-graph cache instance
//! dir, generated `.pando.toml`, process supervision, BIT-US-0141); tests plug in fakes through
//! this trait. Without a supervisor managed mode reports `Unavailable`.

use std::fmt;
use std::path::{Path, PathBuf};

use pando::Token;

use crate::managed::ManagedStatus;

/// Where a supervised Pando instance listens.
#[derive(Clone)]
pub struct ManagedEndpoint {
    /// REST base URL (must be loopback; checked by the service).
    pub rest_url: String,
    /// AG-UI base URL (must be loopback).
    pub agui_url: String,
    /// REST token the supervisor generated or learned for the instance, if any.
    pub rest_token: Option<Token>,
    /// AG-UI token the supervisor generated or learned for the instance, if any.
    pub agui_token: Option<Token>,
    /// PEM of the private CA that signed the instance's certificate (`pando serve` is HTTPS
    /// only); trusted by the REST and AG-UI clients.
    pub ca_pem: Option<Vec<u8>>,
}

impl fmt::Debug for ManagedEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedEndpoint")
            .field("rest_url", &self.rest_url)
            .field("agui_url", &self.agui_url)
            .finish_non_exhaustive()
    }
}

/// Where Pando agents reach Bitacora's MCP server, and with which token (ADR-031). The supervisor
/// writes it into the generated config as `[MCPServers.bitacora]`.
#[derive(Clone, PartialEq, Eq)]
pub struct McpAccess {
    /// Loopback MCP endpoint, for example `http://127.0.0.1:7878/mcp`.
    pub url: String,
    token: String,
}

impl McpAccess {
    /// An endpoint with the secret of the dedicated `pando` token.
    #[must_use]
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            token: token.into(),
        }
    }

    /// The secret (only for writing the private generated config).
    #[must_use]
    pub fn token_str(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for McpAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpAccess")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

/// Starts and stops the managed Pando instance of a graph. Called from a blocking context.
pub trait Supervisor: Send + Sync + fmt::Debug {
    /// Makes sure an instance serves `graph` and returns its endpoint. Idempotent: while the
    /// instance runs it returns the current endpoint, which may differ from the previous call
    /// after a restart.
    ///
    /// # Errors
    /// A message safe to show to the user (binary missing, version too old, port busy...).
    fn ensure_running(&self, graph: &Path) -> Result<ManagedEndpoint, String>;

    /// Stops the instance of `graph` (idempotent).
    fn stop(&self, graph: &Path);

    /// Tells the supervisor how agents reach Bitacora's MCP server; called before
    /// [`ensure_running`](Self::ensure_running). `None` omits the registration.
    fn set_mcp_access(&self, _graph: &Path, _access: Option<McpAccess>) {}

    /// Models to pin extra chat profiles to in the generated config (BIT-US-0180); called before
    /// [`ensure_running`](Self::ensure_running). The config is written at instance start, so a
    /// change takes effect on the next start or [`restart`](Self::restart).
    fn set_chat_models(&self, _graph: &Path, _models: Vec<String>) {}

    /// Status of the instance of `graph`, when this supervisor tracks one.
    fn managed_status(&self, _graph: &Path) -> Option<ManagedStatus> {
        None
    }

    /// Bounces the child of `graph` (also clears a failed state).
    fn restart(&self, _graph: &Path) {}

    /// The instance log of `graph`, when there is one.
    fn log_path(&self, _graph: &Path) -> Option<PathBuf> {
        None
    }
}
