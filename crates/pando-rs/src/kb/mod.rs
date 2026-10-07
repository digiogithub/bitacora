//! Knowledge-base REST API (`/api/v1/remembrances/...`).

mod client;
mod types;

pub use client::KbClient;
pub use types::{
    EmbeddingModel, EmbeddingModels, EmbeddingTest, EmbeddingTestKind, EmbeddingTestResult,
    ReindexStats, RelatedDocument, SearchHit, SearchRequest, SearchResponse, UpsertAction,
    UpsertDocument, UpsertOutcome,
};
