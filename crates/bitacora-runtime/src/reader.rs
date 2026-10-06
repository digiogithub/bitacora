//! [`IndexGraphReader`]: the MCP read side backed by the SQLite index.

use bitacora_index::{IndexStats, ReaderPool};
use bitacora_mcp::{GraphInfo, GraphReader, ReaderError};

/// Reads graph facts from the index through its read-only connection pool.
#[derive(Debug)]
pub struct IndexGraphReader {
    name: String,
    path: String,
    readers: ReaderPool,
}

impl IndexGraphReader {
    /// Reader for the graph at `path` named `name`.
    #[must_use]
    pub fn new(name: String, path: String, readers: ReaderPool) -> Self {
        Self {
            name,
            path,
            readers,
        }
    }
}

impl GraphReader for IndexGraphReader {
    fn graph_info(&self) -> Result<GraphInfo, ReaderError> {
        let conn = self.readers.get().map_err(|e| ReaderError(e.to_string()))?;
        let stats = IndexStats::read(&conn).map_err(|e| ReaderError(e.to_string()))?;
        Ok(GraphInfo {
            name: self.name.clone(),
            path: self.path.clone(),
            page_count: u64::try_from(stats.pages).ok(),
            block_count: u64::try_from(stats.blocks).ok(),
        })
    }
}
