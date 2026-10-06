//! `bitacora-index`: SQLite (FTS5) index, a rebuildable cache of the graph: reindex pipeline, search and the query DSL.
//!
//! This module tree currently provides storage: the per-graph database location
//! ([`IndexLocation`]), the embedded schema v1 ([`schema`]) and the open/validate
//! lifecycle ([`Index::open`]) with a read-only connection pool and a single owned
//! write connection. See `docs/design/sqlite-index-schema.md` §1.1, §3 and §4.1 step 1.

// Reserved for the reindex pipeline (US-0006+); keeps the dependency edge declared.
use bitacora_core as _;

mod error;
mod index;
mod location;
mod pool;
pub mod schema;

pub use error::Error;
pub use index::{
    Index, OpenOptions, OpenOutcome, RebuildKind, RecreateReason, StoredVersions, WriteConnection,
};
pub use location::{IndexLocation, graph_id};
pub use pool::{PooledReader, ReaderPool};

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-index";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
