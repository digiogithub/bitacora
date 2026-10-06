//! Runtime-level sync status stream (BIT-US-0047): a watch cell the UI polls or waits on, with
//! the user-facing message, the retry flag and the backend in use (ADR-020/023).

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use bitacora_sync::GitDetection;
use bitacora_sync::backend::ActiveBackend;
use bitacora_sync::state::SyncStatus;

/// Which git implementation the engine uses, for the sync popover and settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendInfo {
    /// Hybrid (system git + gix) or gix only.
    pub kind: ActiveBackend,
    /// System git executable, when one is used.
    pub git_path: Option<PathBuf>,
    /// System git version, when one was found (also when too old).
    pub git_version: Option<String>,
    /// One line for the UI, e.g. "System git 2.43.0 (/usr/bin/git)" or "Built-in git (gix)".
    pub description: String,
    /// Suggestion shown regardless of failures when the system git is too old or missing.
    pub install_git_suggestion: Option<&'static str>,
}

impl BackendInfo {
    /// Describes the backend chosen for `detection`.
    #[must_use]
    pub fn from_detection(detection: &GitDetection) -> Self {
        match detection {
            GitDetection::Found { path, version } => Self {
                kind: ActiveBackend::Hybrid,
                git_path: Some(path.clone()),
                git_version: Some(version.to_string()),
                description: format!("System git {version} ({})", path.display()),
                install_git_suggestion: None,
            },
            GitDetection::TooOld { path, version } => Self {
                kind: ActiveBackend::GixOnly,
                git_path: Some(path.clone()),
                git_version: Some(version.to_string()),
                description: format!(
                    "Built-in git (gix): system git {version} is older than {}",
                    bitacora_sync::MIN_GIT_VERSION
                ),
                install_git_suggestion: Some("Update git for the best authentication support."),
            },
            GitDetection::Missing => Self {
                kind: ActiveBackend::GixOnly,
                git_path: None,
                git_version: None,
                description: "Built-in git (gix): no system git found".to_owned(),
                install_git_suggestion: Some(bitacora_sync::state::INSTALL_GIT_HINT),
            },
        }
    }
}

/// What the status bar and sync popover render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatusView {
    /// The engine snapshot (state, ahead/behind, last sync, conflicts, last error).
    pub status: SyncStatus,
    /// Short message ("Synced", "3 local commits not synced", "Conflicts (2)", error text).
    pub message: String,
    /// "Install git" suggestion when the built-in backend failed to authenticate.
    pub hint: Option<String>,
    /// Offer a Retry button.
    pub can_retry: bool,
    /// Backend in use.
    pub backend: BackendInfo,
}

impl SyncStatusView {
    fn new(status: SyncStatus, backend: BackendInfo) -> Self {
        Self {
            message: status.user_message(),
            hint: status.backend_hint().map(str::to_owned),
            can_retry: status.can_retry(),
            status,
            backend,
        }
    }
}

struct Cell {
    version: u64,
    view: SyncStatusView,
}

/// Watch channel over the latest [`SyncStatusView`]: cheap to clone, never blocks the engine.
#[derive(Clone)]
pub struct SyncWatch {
    inner: Arc<(Mutex<Cell>, Condvar)>,
    backend: BackendInfo,
}

impl std::fmt::Debug for SyncWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncWatch").finish_non_exhaustive()
    }
}

impl SyncWatch {
    pub(crate) fn new(initial: SyncStatus, backend: BackendInfo) -> Self {
        let view = SyncStatusView::new(initial, backend.clone());
        Self {
            inner: Arc::new((Mutex::new(Cell { version: 0, view }), Condvar::new())),
            backend,
        }
    }

    pub(crate) fn publish(&self, status: &SyncStatus) {
        let view = SyncStatusView::new(status.clone(), self.backend.clone());
        let (cell, cv) = &*self.inner;
        let mut g = cell.lock().unwrap_or_else(PoisonError::into_inner);
        if g.view != view {
            g.version += 1;
            g.view = view;
            cv.notify_all();
        }
    }

    /// The latest view and its version number (starts at 0, grows on every change).
    #[must_use]
    pub fn current(&self) -> (u64, SyncStatusView) {
        let g = self.inner.0.lock().unwrap_or_else(PoisonError::into_inner);
        (g.version, g.view.clone())
    }

    /// Waits until the version differs from `seen` (or `timeout` passes) and returns the latest.
    #[must_use]
    pub fn wait_changed(&self, seen: u64, timeout: Duration) -> (u64, SyncStatusView) {
        let (cell, cv) = &*self.inner;
        let g = cell.lock().unwrap_or_else(PoisonError::into_inner);
        let (g, _) = cv
            .wait_timeout_while(g, timeout, |c| c.version == seen)
            .unwrap_or_else(PoisonError::into_inner);
        (g.version, g.view.clone())
    }
}
