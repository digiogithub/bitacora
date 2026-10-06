//! Read-side abstraction between MCP tools and the graph.
//!
//! The index/core crates implement [`GraphReader`] later; the server only sees this trait, so read
//! tools can be added without the MCP crate depending on concrete index APIs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Summary of the active graph (`get_graph_info` tool).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GraphInfo {
    /// Graph display name.
    pub name: String,
    /// Absolute graph folder path.
    pub path: String,
    /// Number of pages, when known.
    pub page_count: Option<u64>,
    /// Number of blocks, when known.
    pub block_count: Option<u64>,
}

/// Failure reading from the graph.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ReaderError(pub String);

/// Synchronous read access to the graph. Called from `spawn_blocking`, so implementations may block.
pub trait GraphReader: Send + Sync + 'static {
    /// Describe the active graph.
    fn graph_info(&self) -> Result<GraphInfo, ReaderError>;
}

/// Minimal reader that reports a fixed name/path; used by the headless CLI until the index lands.
#[derive(Debug, Clone)]
pub struct StaticGraphReader {
    info: GraphInfo,
}

impl StaticGraphReader {
    /// Create a reader with unknown counts.
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            info: GraphInfo {
                name: name.into(),
                path: path.into(),
                page_count: None,
                block_count: None,
            },
        }
    }
}

impl GraphReader for StaticGraphReader {
    fn graph_info(&self) -> Result<GraphInfo, ReaderError> {
        Ok(self.info.clone())
    }
}
