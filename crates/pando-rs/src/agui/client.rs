//! HTTP side of the AG-UI client: runs (SSE), reattach, cancel and the thread API.

use std::collections::VecDeque;

use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;

use super::sse::SseParser;
use super::types::{Event, Health, Info, Message, RunInput, ThreadsPage};
use crate::client::PandoClient;
use crate::config::Token;
use crate::error::{Error, Result};

/// Route prefix Pando mounts the adapter under by default.
pub const DEFAULT_PATH: &str = "/api/v1/agui";
/// Agent run when none is named.
pub const DEFAULT_AGENT: &str = "coder";

/// Where and as whom to talk to the AG-UI adapter.
#[derive(Debug, Clone)]
pub struct AguiOptions {
    /// Server root when the adapter listens elsewhere than the [`PandoClient`] base URL (a
    /// dedicated `--agui-port` listener). `None` reuses the client's base URL.
    pub base_url: Option<String>,
    /// Route prefix.
    pub path: String,
    /// Default agent for [`AguiClient::run`].
    pub agent: String,
    /// Bearer token. `None` falls back to the client's configured token. The adapter checks
    /// `Authorization: Bearer`, not the REST API's `X-Pando-Token`.
    pub token: Option<Token>,
}

impl Default for AguiOptions {
    fn default() -> Self {
        Self {
            base_url: None,
            path: DEFAULT_PATH.to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            token: None,
        }
    }
}

impl AguiOptions {
    /// Targets a dedicated adapter listener.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets the route prefix.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets the default agent.
    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = agent.into();
        self
    }

    /// Sets a bearer token distinct from the REST token.
    pub fn with_token(mut self, token: impl Into<Token>) -> Self {
        self.token = Some(token.into());
        self
    }
}

/// The AG-UI agent API. Cheap to clone; obtain it from [`PandoClient::agui`].
#[derive(Debug, Clone)]
pub struct AguiClient {
    client: PandoClient,
    base: String,
    path: String,
    agent: String,
    token: Option<Token>,
}

impl AguiClient {
    pub(crate) fn new(client: PandoClient, options: AguiOptions) -> Self {
        let base = options
            .base_url
            .as_deref()
            .map(|b| b.trim().trim_end_matches('/').to_owned())
            .unwrap_or_else(|| client.base().to_owned());
        let path = format!("/{}", options.path.trim().trim_matches('/'));
        let token = options.token.or_else(|| client.config().token.clone());
        Self {
            client,
            base,
            path: if path == "/" { String::new() } else { path },
            agent: options.agent,
            token,
        }
    }

    /// The default agent name.
    pub fn agent(&self) -> &str {
        &self.agent
    }

    /// Discovery document (`GET {path}/info`).
    pub async fn info(&self) -> Result<Info> {
        let resp = self.send(self.req(Method::GET, "/info"), false).await?;
        resp.json().await.map_err(Error::from_transport)
    }

    /// Liveness and concurrency gauge (`GET {path}/healthz`, unauthenticated server-side).
    pub async fn healthz(&self) -> Result<Health> {
        self.json(self.req(Method::GET, "/healthz")).await
    }

    /// Starts a run on the default agent and streams its events.
    pub async fn run(&self, input: &RunInput) -> Result<RunStream> {
        self.run_agent(&self.agent, input).await
    }

    /// Starts a run on `agent` and streams its events.
    ///
    /// The stream ends when the server closes it (after `RUN_FINISHED`). An `interrupt` outcome
    /// means the agent is waiting on the client: answer with a `tool` message and call this again
    /// on the same thread (see [`crate::agui::Thread`]). Dropping the stream detaches without
    /// cancelling the run; use [`AguiClient::cancel_run`] for that.
    pub async fn run_agent(&self, agent: &str, input: &RunInput) -> Result<RunStream> {
        let req = self
            .req(Method::POST, &format!("/{}", encode_segment(agent)))
            .header("Accept", "text/event-stream")
            .json(input);
        RunStream::open(self.send(req, true).await?)
    }

    /// Runs `prompt` on a fresh thread and returns the assistant text. A `RUN_ERROR` becomes
    /// [`Error::Run`]. Tool calls, state and reasoning are dropped; use [`Self::run`] when they
    /// matter.
    pub async fn run_text(&self, prompt: &str) -> Result<String> {
        let mut stream = self.run(&RunInput::new().with_prompt(prompt)).await?;
        let mut text = String::new();
        while let Some(event) = stream.next().await {
            match event? {
                Event::TextMessageContent { delta, .. } => text.push_str(&delta),
                Event::RunError { message, code } => return Err(Error::Run { code, message }),
                _ => {}
            }
        }
        Ok(text)
    }

    /// Reattaches to a thread's live run (`GET {path}/threads/{id}/stream`): the server replays
    /// what was missed, then continues live. `None` when the thread has no live run.
    pub async fn attach(&self, thread_id: &str) -> Result<Option<RunStream>> {
        let req = self
            .req(
                Method::GET,
                &format!("/threads/{}/stream", encode_segment(thread_id)),
            )
            .header("Accept", "text/event-stream");
        match self.send(req, true).await {
            Ok(resp) => RunStream::open(resp).map(Some),
            Err(Error::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Cancels a thread's live or parked run (`POST {path}/runs/{id}/cancel`). Idempotent: a
    /// thread with no live run is a success.
    pub async fn cancel_run(&self, thread_id: &str) -> Result<()> {
        let path = format!("/runs/{}/cancel", encode_segment(thread_id));
        self.send(self.req(Method::POST, &path), false).await?;
        Ok(())
    }

    /// One page of this adapter's threads, newest first. The server caps `limit` at 200.
    pub async fn list_threads(&self, limit: u32, offset: u32) -> Result<ThreadsPage> {
        let req = self
            .req(Method::GET, "/threads")
            .query(&[("limit", limit), ("offset", offset)]);
        self.json(req).await
    }

    /// The thread's persisted transcript; `None` when the server has no such thread.
    pub async fn thread_messages(&self, thread_id: &str) -> Result<Option<Vec<Message>>> {
        #[derive(serde::Deserialize, Default)]
        #[serde(default)]
        struct Body {
            messages: Vec<Message>,
        }
        let path = format!("/threads/{}/messages", encode_segment(thread_id));
        match self.json::<Body>(self.req(Method::GET, &path)).await {
            Ok(body) => Ok(Some(body.messages)),
            Err(Error::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Deletes a thread with its session and messages. Idempotent.
    pub async fn delete_thread(&self, thread_id: &str) -> Result<()> {
        let path = format!("/threads/{}", encode_segment(thread_id));
        self.send(self.req(Method::DELETE, &path), false).await?;
        Ok(())
    }

    fn req(&self, method: Method, suffix: &str) -> RequestBuilder {
        let url = format!("{}{}{}", self.base, self.path, suffix);
        let mut req = self.client.http().request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token.expose());
        }
        req
    }

    async fn json<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T> {
        let resp = self.send(req, false).await?;
        resp.json().await.map_err(Error::from_transport)
    }

    async fn send(&self, req: RequestBuilder, streaming: bool) -> Result<Response> {
        let resp = if streaming {
            // Re-target the built request onto the client without an overall timeout.
            let built = req.build().map_err(Error::from_transport)?;
            self.client.stream_http().execute(built).await
        } else {
            req.send().await
        };
        finish(resp.map_err(Error::from_transport)?).await
    }
}

async fn finish(resp: Response) -> Result<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(map_status(status, &body))
}

fn map_status(status: StatusCode, body: &str) -> Error {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_else(|| body.chars().take(200).collect());
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Error::Unauthorized,
        _ => Error::Server {
            status: status.as_u16(),
            message,
        },
    }
}

/// Percent-encodes one path segment (thread ids and agent names are caller-controlled).
fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for b in segment.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A live SSE stream of [`Event`]s. Pull with [`RunStream::next`]; malformed frames are skipped,
/// unknown event types surface as [`Event::Unknown`].
#[derive(Debug)]
pub struct RunStream {
    resp: Response,
    parser: SseParser,
    queue: VecDeque<Event>,
    done: bool,
}

impl RunStream {
    fn open(resp: Response) -> Result<Self> {
        // A proxy can answer 200 with an HTML/JSON page; that must not look like an empty run.
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !content_type.contains("text/event-stream") {
            return Err(Error::Protocol(format!(
                "AG-UI response was not an SSE stream (Content-Type {content_type:?}); a proxy may have intercepted it"
            )));
        }
        Ok(Self {
            resp,
            parser: SseParser::new(),
            queue: VecDeque::new(),
            done: false,
        })
    }

    /// The next event, `None` when the server closed the stream. A transport failure or idle
    /// timeout yields one `Err` and ends the stream.
    pub async fn next(&mut self) -> Option<Result<Event>> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Some(Ok(event));
            }
            if self.done {
                return None;
            }
            match self.resp.chunk().await {
                Ok(Some(bytes)) => {
                    let payloads = self.parser.push(&bytes);
                    self.enqueue(payloads);
                }
                Ok(None) => {
                    self.done = true;
                    let tail = self.parser.finish();
                    self.enqueue(tail);
                }
                Err(e) => {
                    self.done = true;
                    return Some(Err(Error::from_transport(e)));
                }
            }
        }
    }

    fn enqueue(&mut self, payloads: Vec<String>) {
        for payload in payloads {
            if let Some(event) = Event::parse(&payload) {
                self.queue.push_back(event);
            }
        }
    }
}
