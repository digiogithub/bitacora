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
//! * a `logseq/config.edn` change is reloaded in place (`RuntimeEvent::ConfigReloaded`): core editor
//!   settings, the watcher's `:hidden` rule and the index (full reparse when the config hash
//!   changes) follow it; `Session::reindex` rebuilds the index without reopening;
//! * [`Session::shutdown`] stops everything in order within a time budget.

mod agents;
pub mod crash;
mod glue;
pub mod instance;
mod live;
mod mcp_semantic;
pub mod pando_settings;
mod rename_lookup;
mod restore;
mod session;
mod store;
mod sync_ctl;
mod writer;

pub use bitacora_pando::agents::{
    AgentError, ComposeDeps, ComposeMode, ComposeRequest, run_compose as run_agent_compose,
};
pub use bitacora_pando::semantic::{
    HybridHit, HybridOptions, HybridResults, HybridSearch, HybridTarget, SemanticState, Unavailable,
};
pub use bitacora_pando::{
    ActivityEntry, ActivityKind, ActivityLog, ConnectionReport, KbSharing, ManagedState,
    ManagedStatus, MemoryBackend as PandoMemoryBackend, PandoCredentials, PandoEvent, PandoOptions,
    PandoService, PandoStatus, SecretBackend as PandoSecretBackend, Supervisor, TokenKind,
    TokenSource, kb_sharing, test_connection,
};
/// The AI agent types the app needs (chat events and handles, approvals, the AG-UI thread list),
/// re-exported so `bitacora-app` does not depend on `bitacora-pando` or `pando-rs`.
pub mod ai {
    pub use bitacora_pando::agents::*;
    pub use pando::agui::hitl::{
        PermissionRequest, Question, QuestionAnswer, QuestionAnswerEntry, QuestionOption,
        QuestionRequest,
    };
    pub use pando::agui::{AguiClient, Message, MessageContent, ThreadSummary, ThreadsPage};
    pub use pando::{ModelInfo, ModelList, PandoClient};
}

pub use live::{DEFAULT_SHUTDOWN_BUDGET, Session, ShutdownReport};
pub use pando_settings::{
    PANDO_SETTINGS_FILE, default_pando_settings_path, load_pando_settings, pando_options_from_file,
};
pub use rename_lookup::IndexRefLookup;
pub use restore::RestoreReport;
pub use session::{EngineTune, McpOptions, RuntimeConfig, RuntimeError, RuntimeEvent, SyncOptions};
pub use store::EchoStore;
pub use sync_ctl::{BackendInfo, SyncStatusView, SyncWatch};
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
