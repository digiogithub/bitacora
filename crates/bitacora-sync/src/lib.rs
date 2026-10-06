//! `bitacora-sync`: Git synchronisation: `GitBackend` (system git CLI plus `gix`), the sync loop and conflict state (ADR-007, ADR-020).

use bitacora_core as _;
use bitacora_merge as _;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Placeholder variant until the crate gets real functionality.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-sync";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
        assert_eq!(Error::NotImplemented("x").to_string(), "not implemented: x");
    }
}
