//! Connection settings and credentials.

use std::fmt;
use std::time::Duration;

/// Default overall request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default TCP connect timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default idle timeout of a streaming (SSE) response: the longest silence tolerated between two
/// chunks. Pando sends a keep-alive comment every 15 s, so this leaves room for several misses.
pub const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// An API token. `Debug` never prints the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// Wraps a raw token string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw secret, for building the `X-Pando-Token` header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl From<String> for Token {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Token {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Where and how to reach a Pando server.
#[derive(Debug, Clone)]
pub struct PandoConfig {
    /// Server root, e.g. `http://127.0.0.1:8765`. A trailing slash is ignored.
    pub base_url: String,
    /// API token sent as `X-Pando-Token`. `None` sends no credential.
    pub token: Option<Token>,
    /// Overall per-request timeout.
    pub timeout: Duration,
    /// TCP connect timeout.
    pub connect_timeout: Duration,
    /// Longest silence tolerated on a streaming response (AG-UI runs). Streams have no overall
    /// timeout because a run lasts as long as the agent works.
    pub stream_idle_timeout: Duration,
}

impl PandoConfig {
    /// A config for `base_url` with default timeouts and no token.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token: None,
            timeout: DEFAULT_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            stream_idle_timeout: DEFAULT_STREAM_IDLE_TIMEOUT,
        }
    }

    /// Sets the API token.
    pub fn with_token(mut self, token: impl Into<Token>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Sets the overall request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the connect timeout.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the idle timeout of streaming responses.
    pub fn with_stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = timeout;
        self
    }
}
