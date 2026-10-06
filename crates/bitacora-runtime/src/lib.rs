//! `bitacora-runtime`: a running graph session (ADR-024).
//!
//! Composes `bitacora-core` (command queue, atomic writer), `bitacora-index` (SQLite cache),
//! `bitacora-watch` (external changes with echo suppression), `bitacora-sync` (git) and
//! `bitacora-mcp` into one headless object, [`Session`]. Both `bitacora-app` and `bitacora-cli`
//! depend on it; it depends on no UI crate.
//!
//! Wiring, in short:
//! * every write by core goes through [`EchoStore`], which registers `(path, hash)` with the
//!   watcher's echo filter **before** the rename (BIT-T-0341);
//! * core's `QueueEvent`s feed the index (`Flushed` / `FilesApplied`) and the sync engine's
//!   idle auto-commit (`Flushed`);
//! * the watcher's `FileEvent`s update the index and reload (or drop) the matching loaded page
//!   in core; `Rescan` runs a full reconcile;
//! * [`QueueGraphWriter`] is the sync engine's `GraphWriter` over core's `QueueLock`;
//! * the MCP server reads through `bitacora_mcp::IndexGraphReader` and reports the sync engine's
//!   status; the engine gets index-backed `locate_block` and journal-template hooks;
//! * [`Session::shutdown`] stops everything in order within a time budget.

mod glue;
mod live;
mod rename_lookup;
mod session;
mod store;
mod writer;

pub use live::{DEFAULT_SHUTDOWN_BUDGET, Session, ShutdownReport};
pub use rename_lookup::IndexRefLookup;
pub use session::{EngineTune, McpOptions, RuntimeConfig, RuntimeError, RuntimeEvent, SyncOptions};
pub use store::EchoStore;
pub use writer::{DEFAULT_ACQUIRE_TIMEOUT, QueueGraphWriter};

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-runtime";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
