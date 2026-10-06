//! Git sync status as exposed by `git_sync_status` and `bitacora://sync/status`.
//!
//! The MCP crate does not drive the sync engine; the app or CLI implements [`SyncStatusProvider`]
//! (typically reading the sync engine's watch channel). Without one the status is `disabled`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Coarse sync state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Sync is not configured or turned off.
    #[default]
    Disabled,
    /// Everything is synchronised.
    Idle,
    /// A cycle is running.
    Syncing,
    /// Unresolved conflicts need the user.
    Conflicted,
    /// The last cycle failed.
    Error,
}

/// Snapshot of the sync engine.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct SyncStatus {
    /// Coarse state.
    pub state: SyncState,
    /// Local commits not on the remote.
    pub ahead: u32,
    /// Remote commits not merged locally.
    pub behind: u32,
    /// Last successful sync, unix milliseconds.
    pub last_sync: Option<i64>,
    /// Number of pages with pending conflicts.
    pub conflict_count: u32,
    /// Pages with pending conflicts.
    pub conflict_pages: Vec<String>,
    /// Last error message.
    pub last_error: Option<String>,
}

/// Source of [`SyncStatus`]. Implementations must be cheap and non-blocking.
pub trait SyncStatusProvider: Send + Sync + 'static {
    /// Current status.
    fn status(&self) -> SyncStatus;
}

/// Provider used when no sync engine is attached.
#[derive(Debug, Clone, Copy, Default)]
pub struct DisabledSync;

impl SyncStatusProvider for DisabledSync {
    fn status(&self) -> SyncStatus {
        SyncStatus::default()
    }
}
