//! Authenticated HTTP transport shared by every Pando API client.

use std::sync::Arc;

use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::PandoConfig;
use crate::error::{Error, Result};
use crate::kb::KbClient;

/// What `GET /health` reports about the server (unknown fields are ignored).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServerInfo {
    /// Server version string.
    pub version: String,
    /// Startup mode (for example `project-child`), empty when unset.
    pub startup_mode: String,
}

#[derive(Debug)]
struct Inner {
    http: reqwest::Client,
    base: String,
    config: PandoConfig,
}

/// Entry point of the SDK. Cheap to clone; clones share one connection pool.
#[derive(Debug, Clone)]
pub struct PandoClient {
    inner: Arc<Inner>,
}

impl PandoClient {
    /// Builds a client from `config`.
    pub fn new(config: PandoConfig) -> Result<Self> {
        let base = config.base_url.trim().trim_end_matches('/').to_owned();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(Error::Config(format!(
                "base_url must start with http:// or https://, got {:?}",
                config.base_url
            )));
        }
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .build()
            .map_err(|e| Error::Config(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(Inner { http, base, config }),
        })
    }

    /// The knowledge-base REST API.
    pub fn kb(&self) -> KbClient {
        KbClient::new(self.clone())
    }

    /// Server version info from `GET /health` (unauthenticated on the server side).
    pub async fn info(&self) -> Result<ServerInfo> {
        self.send_json(self.request(Method::GET, "/health")).await
    }

    pub(crate) fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.inner.base, path);
        let mut req = self.inner.http.request(method, url);
        if let Some(token) = &self.inner.config.token {
            req = req.header("X-Pando-Token", token.expose());
        }
        req
    }

    /// Sends and decodes a JSON answer.
    pub(crate) async fn send_json<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T> {
        let resp = self.send(req).await?;
        resp.json::<T>().await.map_err(Error::from_transport)
    }

    /// Sends and maps non-success statuses to [`Error`].
    pub(crate) async fn send(&self, req: RequestBuilder) -> Result<Response> {
        let resp = req.send().await.map_err(Error::from_transport)?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        Err(map_status(status, &body))
    }
}

fn map_status(status: StatusCode, body: &str) -> Error {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_else(|| body.chars().take(200).collect());
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Error::Unauthorized,
        StatusCode::SERVICE_UNAVAILABLE => Error::NotConfigured(message),
        _ => Error::Server {
            status: status.as_u16(),
            message,
        },
    }
}
