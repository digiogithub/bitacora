//! `bitacora-mcp`: MCP server over Streamable HTTP (loopback only, bearer token, audited undoable writes; ADR-010).
//!
//! Layout:
//! - [`McpServer`] / [`McpConfig`]: lifecycle on a dedicated tokio runtime, usable from synchronous callers.
//! - [`GraphReader`]: the read-side trait the index/core will implement; tools plug in behind it.
//! - [`TokenStore`]: bearer-token generation, storage (0600 file in the app config dir), rotation.
//! - `guard` (private): Host / Origin / bearer-token middleware; every request passes it except `GET /health`
//!   which skips only the token check.
//!
//! `rmcp` types are confined to the private `handler` and `server` modules so a future rmcp major
//! touches one place (design `mcp-server.md` section 1).

mod audit;
mod bridge;
mod compat;
mod dates;
mod exclusion;
mod guard;
mod handler;
mod index_reader;
mod policy;
mod prompts;
mod query;
mod reader;
mod render;
mod resources;
mod server;
mod status;
mod tokens;
mod tools;
mod write_tools;

pub use audit::{AuditEvent, AuditFilter, AuditLog, AuditRecord, UndoError};
pub use bridge::QueueBridge;
pub use exclusion::{FilteredReader, PRIVATE_PROPERTY, ReadExclusions};
pub use index_reader::IndexGraphReader;
pub use policy::{DEFAULT_WRITES_PER_MINUTE, OpenGate, WriteGate, WritePolicy};
pub use reader::{
    BlockInfo, ChangeEvent, GraphInfo, GraphReader, ListPagesQuery, PageInfo, QueryOutcome,
    QueryRequest, ReaderError, ReaderErrorKind, ReaderResult, RefGroupInfo, RefItem, SearchItem,
    SearchKind, SearchQuery, StaticGraphReader, TaskQuery,
};
pub use server::{DEFAULT_PORT, McpConfig, McpServer, ServerParts};
pub use status::{DisabledSync, SyncState, SyncStatus, SyncStatusProvider};
#[cfg(feature = "keyring-store")]
pub use tokens::KeyringBackend;
pub use tokens::{
    MemoryBackend, PANDO_TOKEN_NAME, Scope, SecretBackend, TokenInfo, TokenStorage, TokenStore,
    TokenSummary, default_audit_dir, default_token_path, os_keychain,
};

use bitacora_core as _;
use bitacora_index as _;
use bitacora_sync as _;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configured bind address is not a loopback address (ADR-010).
    #[error("refusing to bind non-loopback address {0}; the MCP server is loopback-only")]
    NonLoopbackBind(std::net::IpAddr),
    /// Another process already listens on the configured port; no fallback port is used.
    #[error("MCP port {0} is already in use")]
    PortInUse(u16),
    /// Any other I/O failure (bind, runtime creation, token file).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The token file exists but cannot be understood; it is never silently regenerated.
    #[error("token file {path} is corrupt: {message}")]
    TokenFile {
        /// Path of the offending file.
        path: std::path::PathBuf,
        /// Parser message.
        message: String,
    },
    /// A token with this name already exists.
    #[error("a token named `{0}` already exists")]
    TokenExists(String),
    /// No token with this name exists.
    #[error("no token named `{0}`")]
    TokenNotFound(String),
    /// The operating system random source failed.
    #[error("random source unavailable: {0}")]
    Random(String),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-mcp";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
        assert_eq!(
            Error::PortInUse(1).to_string(),
            "MCP port 1 is already in use"
        );
    }
}
