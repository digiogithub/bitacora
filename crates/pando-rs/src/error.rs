//! Error type shared by every Pando API.

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong talking to Pando.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The server answered 503: the feature (KB store, embedder, mirror) is not set up.
    #[error("pando feature not configured: {0}")]
    NotConfigured(String),
    /// 401/403: the token is missing or wrong.
    #[error("pando rejected the credentials")]
    Unauthorized,
    /// The server could not be reached (connection refused, DNS, TLS).
    #[error("pando is unreachable: {0}")]
    Unreachable(String),
    /// The request timed out.
    #[error("pando request timed out")]
    Timeout,
    /// A KB reindex is already running (409 from the reindex route).
    #[error("a pando KB reindex is already running")]
    ReindexRunning,
    /// Any other non-success answer.
    #[error("pando server error {status}: {message}")]
    Server {
        /// HTTP status code.
        status: u16,
        /// Server-provided message (`{"error": ...}`) or the raw body prefix.
        message: String,
    },
    /// The response was not the JSON shape we expected.
    #[error("invalid pando response: {0}")]
    Decode(String),
    /// The configuration is unusable (bad base URL, TLS init failure).
    #[error("invalid pando configuration: {0}")]
    Config(String),
}

impl Error {
    pub(crate) fn from_transport(err: reqwest::Error) -> Self {
        let err = err.without_url();
        if err.is_timeout() {
            Self::Timeout
        } else if err.is_decode() {
            Self::Decode(err.to_string())
        } else {
            Self::Unreachable(err.to_string())
        }
    }
}
