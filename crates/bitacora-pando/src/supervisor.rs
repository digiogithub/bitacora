//! Seam for managed mode (ADR-029): who runs `pando serve` for a graph.
//!
//! The service only needs "give me a loopback endpoint (and tokens) for this graph" and "stop it".
//! The real implementation (per-graph cache instance dir, generated `.pando.toml`, process
//! supervision) is BIT-US-0141 and plugs in through [`Supervisor`]; until then managed mode
//! reports `Unavailable`.

use std::fmt;
use std::path::Path;

use pando::Token;

/// Where a supervised Pando instance listens.
#[derive(Clone)]
pub struct ManagedEndpoint {
    /// REST base URL (must be loopback; checked by the service).
    pub rest_url: String,
    /// AG-UI base URL (must be loopback).
    pub agui_url: String,
    /// REST token the supervisor generated for the instance, if any.
    pub rest_token: Option<Token>,
    /// AG-UI token the supervisor generated for the instance, if any.
    pub agui_token: Option<Token>,
}

impl fmt::Debug for ManagedEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedEndpoint")
            .field("rest_url", &self.rest_url)
            .field("agui_url", &self.agui_url)
            .finish_non_exhaustive()
    }
}

/// Starts and stops the managed Pando instance of a graph. Called from a blocking context.
pub trait Supervisor: Send + Sync + fmt::Debug {
    /// Makes sure an instance serves `graph` and returns its endpoint.
    ///
    /// # Errors
    /// A message safe to show to the user (binary missing, version too old, port busy...).
    fn ensure_running(&self, graph: &Path) -> Result<ManagedEndpoint, String>;

    /// Stops the instance of `graph` (idempotent).
    fn stop(&self, graph: &Path);
}
