use reqwest::Method;
use serde_json::json;

use super::types::{
    EmbeddingModels, EmbeddingTest, EmbeddingTestKind, ReindexStats, SearchRequest, SearchResponse,
    UpsertDocument, UpsertOutcome,
};
use crate::client::PandoClient;
use crate::error::{Error, Result};

const BASE: &str = "/api/v1/remembrances";

/// Typed client for Pando's REST knowledge-base routes.
#[derive(Debug, Clone)]
pub struct KbClient {
    client: PandoClient,
}

impl KbClient {
    pub(crate) fn new(client: PandoClient) -> Self {
        Self { client }
    }

    /// `POST /kb/documents`: create or replace a document.
    pub async fn upsert(&self, doc: &UpsertDocument) -> Result<UpsertOutcome> {
        let req = self
            .client
            .request(Method::POST, &format!("{BASE}/kb/documents"))
            .json(doc);
        self.client.send_json(req).await
    }

    /// `DELETE /kb/documents`: remove a document and its mirrored file.
    pub async fn delete(&self, file_path: &str) -> Result<()> {
        let req = self
            .client
            .request(Method::DELETE, &format!("{BASE}/kb/documents"))
            .json(&json!({ "file_path": file_path }));
        self.client.send(req).await.map(drop)
    }

    /// `POST /kb/search`: hybrid (vector + full-text) search.
    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResponse> {
        let req = self
            .client
            .request(Method::POST, &format!("{BASE}/kb/search"))
            .json(request);
        self.client.send_json(req).await
    }

    /// `POST /kb/reindex`: re-sync the server's filesystem mirror into the index.
    /// A reindex already in progress (409) maps to [`Error::ReindexRunning`].
    pub async fn reindex(&self) -> Result<ReindexStats> {
        let req = self
            .client
            .request(Method::POST, &format!("{BASE}/kb/reindex"));
        match self.client.send_json(req).await {
            Err(Error::Server { status: 409, .. }) => Err(Error::ReindexRunning),
            other => other,
        }
    }

    /// `GET /embedding-models`: models offered by `provider` (or the configured one).
    pub async fn embedding_models(&self, provider: Option<&str>) -> Result<EmbeddingModels> {
        let mut req = self
            .client
            .request(Method::GET, &format!("{BASE}/embedding-models"));
        if let Some(p) = provider {
            req = req.query(&[("provider", p)]);
        }
        self.client.send_json(req).await
    }

    /// `POST /test-connection`: exercise the configured embedder(s).
    pub async fn test_embedding(&self, kind: EmbeddingTestKind) -> Result<EmbeddingTest> {
        let req = self
            .client
            .request(Method::POST, &format!("{BASE}/test-connection"))
            .json(&json!({ "type": kind.as_str() }));
        self.client.send_json(req).await
    }
}
