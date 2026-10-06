//! `bitacora-core`: Graph model, title/path mapping, `Op` transactions with undo, the single-writer command queue and atomic file writer. Synchronous and executor-agnostic (no tokio, ADR-012).

pub mod date;
pub mod editor;
pub mod graph;
pub mod graph_path;
pub mod journal;
pub mod naming;
pub mod queue;
pub mod scan;
pub mod write_queue;

use bitacora_markdown as _;
use bitacora_merge as _;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Placeholder variant until the crate gets real functionality.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-core";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
        assert_eq!(Error::NotImplemented("x").to_string(), "not implemented: x");
    }
}
