//! Host / Origin / bearer-token guard (BIT-SP-0007.R3, R4, R5; DNS-rebinding protection).
//!
//! Order: Host (403) -> Origin (403) -> bearer token (401). `GET /health` skips only the token
//! check. No CORS headers are ever emitted, and `OPTIONS` preflights are not special-cased: a browser
//! preflight carries an `Origin`, so it is rejected like any other foreign request.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::audit::AuditLog;
use crate::tokens::TokenStore;

/// State shared by the guard middleware.
#[derive(Clone)]
pub(crate) struct GuardState {
    pub port: u16,
    pub allowed_origins: Arc<Vec<String>>,
    pub tokens: Arc<TokenStore>,
    pub audit: Arc<AuditLog>,
}

/// `Host` must be a loopback authority on the bound port.
pub(crate) fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|h| host == format!("{h}:{port}"))
}

fn normalize_origin(o: &str) -> String {
    o.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// An absent `Origin` (native clients) passes; a present one must be in the allowlist. The literal
/// `null` origin is rejected unless explicitly allowlisted.
pub(crate) fn origin_allowed(origin: Option<&str>, allowlist: &[String]) -> bool {
    match origin {
        None => true,
        Some(o) => {
            let o = normalize_origin(o);
            allowlist.iter().any(|a| normalize_origin(a) == o)
        }
    }
}

/// Extract the secret from `Authorization: Bearer <token>` (scheme is case-insensitive).
pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, rest) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim();
    (!token.is_empty()).then_some(token)
}

fn forbidden(msg: &'static str) -> Response {
    (StatusCode::FORBIDDEN, msg).into_response()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer realm=\"bitacora\"")],
        "unauthorized",
    )
        .into_response()
}

/// The axum middleware function.
pub(crate) async fn guard(
    State(state): State<GuardState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let headers = req.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if !host_allowed(host, state.port) {
        return forbidden("forbidden host");
    }
    // A non-UTF-8 Origin cannot match anything: treat it as present-and-foreign.
    let origin = match headers.get(header::ORIGIN) {
        None => None,
        Some(v) => Some(v.to_str().unwrap_or("\u{0}invalid")),
    };
    if !origin_allowed(origin, &state.allowed_origins) {
        return forbidden("forbidden origin");
    }
    let is_health = req.method() == Method::GET && req.uri().path() == "/health";
    if !is_health {
        let path = req.uri().path().to_owned();
        let Some(token) = bearer(req.headers()) else {
            state
                .audit
                .record_auth_failure(&format!("missing bearer token for {path}"));
            return unauthorized();
        };
        let Some(info) = state.tokens.verify(token) else {
            state
                .audit
                .record_auth_failure(&format!("invalid bearer token for {path}"));
            return unauthorized();
        };
        // Downstream handlers (audit, scope checks) can read who is calling.
        req.extensions_mut().insert(info);
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_rules() {
        assert!(host_allowed(Some("127.0.0.1:12316"), 12316));
        assert!(host_allowed(Some("LocalHost:12316"), 12316));
        assert!(host_allowed(Some("[::1]:12316"), 12316));
        assert!(!host_allowed(Some("127.0.0.1:1"), 12316));
        assert!(!host_allowed(Some("127.0.0.1"), 12316));
        assert!(!host_allowed(Some("evil.example:12316"), 12316));
        assert!(!host_allowed(Some("localhost.evil.example:12316"), 12316));
        assert!(!host_allowed(None, 12316));
    }

    #[test]
    fn origin_rules() {
        let allow = vec!["http://localhost:6274/".to_owned()];
        assert!(origin_allowed(None, &[]));
        assert!(!origin_allowed(Some("http://evil.example"), &[]));
        assert!(!origin_allowed(Some("null"), &allow));
        assert!(origin_allowed(Some("http://localhost:6274"), &allow));
        assert!(origin_allowed(Some("HTTP://LOCALHOST:6274"), &allow));
        assert!(!origin_allowed(Some("http://localhost:6275"), &allow));
    }

    #[test]
    fn bearer_parsing() {
        let mut h = HeaderMap::new();
        assert!(bearer(&h).is_none());
        h.insert(header::AUTHORIZATION, "Bearer abc".parse().expect("hv"));
        assert_eq!(bearer(&h), Some("abc"));
        h.insert(header::AUTHORIZATION, "bearer abc".parse().expect("hv"));
        assert_eq!(bearer(&h), Some("abc"));
        h.insert(header::AUTHORIZATION, "Basic abc".parse().expect("hv"));
        assert!(bearer(&h).is_none());
        h.insert(header::AUTHORIZATION, "Bearer ".parse().expect("hv"));
        assert!(bearer(&h).is_none());
    }
}
