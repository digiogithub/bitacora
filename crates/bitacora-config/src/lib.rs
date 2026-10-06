//! `bitacora-config`: Logseq `config.edn` reading with comment-preserving edits, and Bitacora application settings.
//!
//! - [`cst`]: lossless EDN tree (every byte kept).
//! - [`edn`]: value model and reader with duplicate-key detection.
//! - [`config`] and [`accessors`]: effective config (defaults, global, graph) and typed keys.
//! - [`edit`]: surgical, comment-preserving edits returning new text.

pub mod accessors;
pub mod config;
pub mod cst;
pub mod edit;
pub mod edn;
pub mod error;

pub use accessors::{
    BulletIndentation, DefaultHome, NameFormat, PreferredFormat, PreferredWorkflow,
};
pub use config::{
    DEFAULT_CONFIG_EDN, EffectiveConfig, default_config, global_config_path, merge_configs,
};
pub use cst::Cst;
pub use edit::ConfigEditor;
pub use edn::{Edn, read_str};
pub use error::{Diagnostic, DiagnosticKind, Error};

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-config";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
