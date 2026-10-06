//! `bitacora-index`: SQLite (FTS5) index, a rebuildable cache of the graph: reindex pipeline, search and the query DSL.
//!
//! Storage, the single-writer transactional replace (`replace`, `writer`) and the reindex
//! pipeline (`reconcile`) live here. Storage: the per-graph database location
//! ([`IndexLocation`]), the embedded schema v1 ([`schema`]) and the open/validate
//! lifecycle ([`Index::open`]) with a read-only connection pool and a single owned
//! write connection. See `docs/design/sqlite-index-schema.md` §1.1, §3 and §4.1 step 1.

mod carry;
mod config_hash;
mod diagnostics;
pub mod dump;
mod error;
mod index;
mod location;
pub mod normalize;
pub mod parse;
pub mod parsed;
mod pool;
pub mod read;
mod reconcile;
mod replace;
pub mod schema;
pub mod search;
mod writer;

pub use carry::{OldBlock, assign_uuids};
pub use config_hash::config_hash;
pub use diagnostics::{CheckResult, DoctorReport, IndexStats, inspect_index};
pub use error::Error;
pub use index::{
    Index, OpenOptions, OpenOutcome, RebuildKind, RecreateReason, StoredVersions, WriteConnection,
};
pub use location::{IndexLocation, graph_id};
pub use parse::{PARSER_VERSION, ParseConfig, parse};
pub use parsed::*;
pub use pool::{PooledReader, ReaderPool};
pub use read::{
    AgendaItem, AgendaKind, BlockRow, Crumb, DiagnosticCount, DiagnosticFilter, DiagnosticRow,
    GraphEdge, GraphEdgeKind, GraphNode, GraphOptions, GraphView, IndexReader, NamespaceNode,
    PageFilter, PageRow, PageSort, RefFilters, RefGroup, RefHit, TaskFilter, TaskItem,
};
pub use reconcile::{FsChange, Indexer, IndexerOptions, ReconcileStats};
pub use replace::{
    BUILTIN_PAGES, DeleteOutcome, FileInput, FileKind, ReplaceOutcome, WriteOptions, delete_file,
    page_uuid, replace_file, seed_builtin_pages,
};
pub use writer::{IndexEvent, IndexWriter, Pending};

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
