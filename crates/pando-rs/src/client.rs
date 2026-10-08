//! Authenticated HTTP transport shared by every Pando API client.

use std::sync::Arc;

use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::agui::{AguiClient, AguiOptions};
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

/// One model of `GET /api/v1/models` (unknown fields are ignored).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelInfo {
    /// Model id, as accepted by a profile's `Model` setting.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Provider type.
    pub provider: String,
    /// Provider account the model came from, when the server has several.
    pub account_id: String,
    /// Context window in tokens, `0` when unknown.
    pub context_window: i64,
    /// The model can reason.
    pub can_reason: bool,
}

/// `GET /api/v1/models`: every model of every configured provider account.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelList {
    /// The models (the synthetic `auto` entry, when present, comes first).
    pub models: Vec<ModelInfo>,
    /// Per-account listing errors (account id to message).
    pub errors: std::collections::BTreeMap<String, String>,
    /// The server's automatic model routing is on.
    pub auto_selected: bool,
}

#[derive(Debug)]
struct Inner {
    http: reqwest::Client,
    /// Same pool settings but no overall timeout, only a per-read idle timeout (SSE streams).
    stream_http: reqwest::Client,
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
        let roots = match &config.root_certificate_pem {
            Some(pem) => reqwest::Certificate::from_pem_bundle(pem)
                .map_err(|e| Error::Config(format!("invalid root certificate: {e}")))?,
            None => Vec::new(),
        };
        let with_roots = |mut b: reqwest::ClientBuilder| {
            for cert in &roots {
                b = b.add_root_certificate(cert.clone());
            }
            b
        };
        let http = with_roots(
            reqwest::Client::builder()
                .timeout(config.timeout)
                .connect_timeout(config.connect_timeout),
        )
        .build()
        .map_err(|e| Error::Config(e.to_string()))?;
        let stream_http = with_roots(
            reqwest::Client::builder()
                .read_timeout(config.stream_idle_timeout)
                .connect_timeout(config.connect_timeout),
        )
        .build()
        .map_err(|e| Error::Config(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                stream_http,
                base,
                config,
            }),
        })
    }

    /// The knowledge-base REST API.
    pub fn kb(&self) -> KbClient {
        KbClient::new(self.clone())
    }

    /// The AG-UI agent API with default options (`/api/v1/agui`, agent `coder`, same server).
    pub fn agui(&self) -> AguiClient {
        AguiClient::new(self.clone(), AguiOptions::default())
    }

    /// The AG-UI agent API with explicit options (dedicated listener, other agent or token).
    pub fn agui_with(&self, options: AguiOptions) -> AguiClient {
        AguiClient::new(self.clone(), options)
    }

    /// Server version info from `GET /health` (unauthenticated on the server side).
    pub async fn info(&self) -> Result<ServerInfo> {
        self.send_json(self.request(Method::GET, "/health")).await
    }

    /// Models available on the server, from `GET /api/v1/models`.
    pub async fn list_models(&self) -> Result<ModelList> {
        self.send_json(self.request(Method::GET, "/api/v1/models"))
            .await
    }

    /// The API token of a local `pando serve` from `GET /api/v1/token`. Pando answers it without
    /// credentials only on a loopback bind, which is how a supervisor learns the token of the
    /// server it just started.
    pub async fn fetch_api_token(&self) -> Result<crate::config::Token> {
        #[derive(Deserialize)]
        struct Body {
            token: String,
        }
        let body: Body = self
            .send_json(self.request(Method::GET, "/api/v1/token"))
            .await?;
        Ok(crate::config::Token::new(body.token))
    }

    pub(crate) fn base(&self) -> &str {
        &self.inner.base
    }

    pub(crate) fn config(&self) -> &PandoConfig {
        &self.inner.config
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    pub(crate) fn stream_http(&self) -> &reqwest::Client {
        &self.inner.stream_http
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
