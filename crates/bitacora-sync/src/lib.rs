//! `bitacora-sync`: Git synchronisation: `GitBackend` (system git CLI plus `gix`), repository
//! setup and onboarding, the sync loop and conflict state (ADR-007, ADR-020).

pub mod askpass;
pub mod autocommit;
pub mod backend;
pub mod commit_msg;
pub mod credentials;
pub mod engine;
pub mod merge;
pub mod onboarding;
pub mod repo_setup;
pub mod state;
pub mod writer;

use bitacora_core as _;
use bitacora_merge as _;

pub use backend::{
    ActiveBackend, CliBackend, CliConfig, CommitKind, CommitMessage, CommitOpts, FakeBackend,
    FetchOutcome, GitBackend, GitDetection, GitError, GitVersion, GixBackend, HybridBackend,
    MIN_GIT_VERSION, Oid, PushOutcome, RepoStatus, TreeChange, TreeEdit, detect_git,
    select_backend,
};

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-sync";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
