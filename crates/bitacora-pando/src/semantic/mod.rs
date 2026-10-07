//! Semantic indexing of the graph in Pando's knowledge base (ADR-030, BIT-US-0142/0143).
//!
//! - [`doc`]: the deterministic block -> document mapping and the [`ContentPolicy`].
//! - [`ledger`]: the machine-local sync ledger and durable outbox (outside the rebuildable index).
//! - [`source`]: [`DocSource`], the eligible documents, read from the index.
//! - [`worker`]: [`SemanticWorker`], which follows `IndexWriter` events, diffs content hashes into
//!   the outbox and sends per-document upserts and deletes with bounded concurrency.
//!
//! Only Pando's generic REST API is used (`POST`/`DELETE /api/v1/remembrances/kb/documents`).

pub mod doc;
pub mod ledger;
pub mod session;
pub mod source;
pub mod worker;

pub use doc::{
    ContentPolicy, DEFAULT_MIN_CHARS, DOC_ID_ROOT, DOC_SCHEMA, GraphInfo, SemanticDoc, Skip,
    doc_id, doc_prefix, map_block,
};
pub use ledger::{Ledger, Op, OutboxEntry, StateEntry};
pub use session::{LEDGER_FILE, SessionInputs, start_session};
pub use source::{DocSource, IndexSource, SharedPolicy};
pub use worker::{
    Gate, KbProvider, Reconciler, SemanticParams, SemanticStatus, SemanticWorker, SenderConfig,
    attach,
};

/// Failures of the semantic indexer.
#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    /// The index could not be read.
    #[error("index: {0}")]
    Index(String),
    /// The ledger database failed.
    #[error("ledger: {0}")]
    Ledger(#[from] rusqlite::Error),
    /// A file-system operation failed.
    #[error("i/o: {0}")]
    Io(String),
    /// A worker thread could not start.
    #[error("worker: {0}")]
    Worker(String),
}

impl From<bitacora_index::Error> for SemanticError {
    fn from(e: bitacora_index::Error) -> Self {
        Self::Index(e.to_string())
    }
}
