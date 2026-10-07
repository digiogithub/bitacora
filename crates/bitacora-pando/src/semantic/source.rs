//! Where documents come from: the index, behind a small trait so the worker is testable.

use std::sync::Arc;

use bitacora_index::IndexReader;
use parking_lot::RwLock;

use super::SemanticError;
use super::doc::{ContentPolicy, GraphInfo, SemanticDoc, map_block};

/// The content policy, shared between the worker (which may replace it when the user edits the
/// exclusions) and the source (which applies it).
pub type SharedPolicy = Arc<RwLock<ContentPolicy>>;

/// Eligible documents of a graph.
pub trait DocSource: Send + Sync {
    /// Graph-relative paths of every file that can hold documents.
    ///
    /// # Errors
    /// The source could not be read.
    fn file_paths(&self) -> Result<Vec<String>, SemanticError>;

    /// The eligible documents of the file at `path` (empty when the file is gone).
    ///
    /// # Errors
    /// The source could not be read.
    fn file_docs(&self, path: &str) -> Result<Vec<SemanticDoc>, SemanticError>;

    /// The document of one block, `None` when the block is gone or not eligible.
    ///
    /// # Errors
    /// The source could not be read.
    fn block_doc(&self, uuid: &str) -> Result<Option<SemanticDoc>, SemanticError>;
}

/// [`DocSource`] over the SQLite index.
#[derive(Debug, Clone)]
pub struct IndexSource {
    reader: IndexReader,
    graph: GraphInfo,
    policy: SharedPolicy,
}

impl IndexSource {
    /// Documents of `graph` read through `reader` and filtered by `policy`.
    #[must_use]
    pub fn new(reader: IndexReader, graph: GraphInfo, policy: SharedPolicy) -> Self {
        Self {
            reader,
            graph,
            policy,
        }
    }
}

impl DocSource for IndexSource {
    fn file_paths(&self) -> Result<Vec<String>, SemanticError> {
        Ok(self.reader.semantic_file_paths()?)
    }

    fn file_docs(&self, path: &str) -> Result<Vec<SemanticDoc>, SemanticError> {
        let policy = self.policy.read().clone();
        Ok(self
            .reader
            .semantic_blocks(path)?
            .iter()
            .filter_map(|b| map_block(&self.graph, b, &policy).ok())
            .collect())
    }

    fn block_doc(&self, uuid: &str) -> Result<Option<SemanticDoc>, SemanticError> {
        let policy = self.policy.read().clone();
        Ok(self
            .reader
            .semantic_block(uuid)?
            .and_then(|b| map_block(&self.graph, &b, &policy).ok()))
    }
}
