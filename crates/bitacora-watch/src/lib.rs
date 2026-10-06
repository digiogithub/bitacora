//! `bitacora-watch`: File-system watcher built on `notify` with debouncing and echo suppression of our own writes.

use bitacora_core as _;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Placeholder variant until the crate gets real functionality.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-watch";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
        assert_eq!(Error::NotImplemented("x").to_string(), "not implemented: x");
    }
}
