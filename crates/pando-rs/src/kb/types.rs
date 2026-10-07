//! Request/response shapes. Responses tolerate unknown and missing fields.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Body of `POST /kb/documents`.
#[derive(Debug, Clone, Serialize)]
pub struct UpsertDocument {
    /// Document key (path-like, unique).
    pub file_path: String,
    /// Full Markdown content.
    pub content: String,
    /// Optional metadata. `None` keeps the stored metadata of an existing document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
    /// Tags, stored under `metadata["tags"]` by the server.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl UpsertDocument {
    /// A document with no metadata and no tags.
    pub fn new(file_path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            content: content.into(),
            metadata: None,
            tags: Vec::new(),
        }
    }
}

/// Whether an upsert created or updated the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertAction {
    /// The document did not exist.
    Created,
    /// The document existed and was replaced.
    Updated,
    /// A value this SDK does not know.
    Other,
}

impl<'de> Deserialize<'de> for UpsertAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "created" => Self::Created,
            "updated" => Self::Updated,
            _ => Self::Other,
        })
    }
}

/// Answer of an upsert.
#[derive(Debug, Clone, Deserialize)]
pub struct UpsertOutcome {
    /// Echo of the document key.
    #[serde(default)]
    pub file_path: String,
    /// Created or updated.
    pub action: UpsertAction,
}

/// Body of `POST /kb/search`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchRequest {
    /// Query text (required, non-blank).
    pub query: String,
    /// Result count; the server defaults to 5 and caps at 20.
    pub limit: u32,
    /// Restrict to documents carrying these tags.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Restrict to a path prefix.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path_prefix: String,
    /// Server-defined scope selector.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scope: String,
    /// Sort by date instead of relevance.
    pub sort_by_date: bool,
    /// `None` uses the server default (true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_outdated: Option<bool>,
}

impl SearchRequest {
    /// A search for `query` returning up to `limit` hits.
    pub fn new(query: impl Into<String>, limit: u32) -> Self {
        Self {
            query: query.into(),
            limit,
            ..Self::default()
        }
    }
}

/// One search result chunk.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SearchHit {
    /// Document key.
    pub file_path: String,
    /// Matching chunk text.
    pub chunk_content: String,
    /// Relevance score.
    pub score: f64,
    /// 1-based rank.
    pub rank: u32,
    /// Document tags.
    pub tags: Vec<String>,
    /// RFC 3339 creation time.
    pub created_at: String,
    /// RFC 3339 update time.
    pub updated_at: String,
    /// Stored metadata.
    pub metadata: Map<String, Value>,
    /// Outgoing wiki links.
    pub links: u32,
    /// Incoming wiki links.
    pub backlinks: u32,
}

/// A graph neighbour of the top result.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RelatedDocument {
    /// Document key.
    pub file_path: String,
    /// Relatedness score.
    pub score: f64,
    /// Why it is related.
    pub reasons: Vec<String>,
}

/// Answer of a search.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SearchResponse {
    /// Number of hits.
    pub count: u32,
    /// The hits.
    pub results: Vec<SearchHit>,
    /// Non-fatal server warning (for example stale embeddings).
    pub warning: Option<String>,
    /// Graph neighbours of the top hit.
    pub related_to_top_result: Vec<RelatedDocument>,
}

/// Outcome of a KB reindex.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReindexStats {
    /// Files looked at.
    pub scanned: u32,
    /// New documents.
    pub added: u32,
    /// Changed documents.
    pub updated: u32,
    /// Unchanged documents.
    pub unchanged: u32,
    /// Removed documents.
    pub deleted: u32,
    /// Wiki links written by this run.
    pub links_indexed: u32,
}

/// An embedding model offered by a provider.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EmbeddingModel {
    /// Model id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Human-readable size, when reported.
    pub size: String,
}

/// Answer of `GET /embedding-models`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EmbeddingModels {
    /// Provider queried.
    pub provider: String,
    /// Models found.
    pub models: Vec<EmbeddingModel>,
    /// `api`, `heuristic` or `static`.
    pub source: String,
    /// Provider error text, when the live query failed.
    pub error: String,
}

/// Which embedder `test-connection` should exercise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingTestKind {
    /// Document embedder.
    Document,
    /// Code embedder.
    Code,
    /// Both.
    All,
}

impl EmbeddingTestKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Code => "code",
            Self::All => "all",
        }
    }
}

/// Result of testing one embedder.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EmbeddingTestResult {
    /// Whether the embedder answered.
    pub ok: bool,
    /// Failure text.
    pub error: String,
    /// Round-trip latency.
    pub latency_ms: i64,
    /// Vector dimension.
    pub dimension: u32,
    /// Provider name.
    pub provider: String,
    /// Model name.
    pub model: String,
}

/// Answer of `POST /test-connection`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EmbeddingTest {
    /// Document embedder result, when tested.
    pub document: Option<EmbeddingTestResult>,
    /// Code embedder result, when tested.
    pub code: Option<EmbeddingTestResult>,
}
