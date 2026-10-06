#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! HTTP integration tests: a real axum server on an ephemeral port, driven by a raw HTTP/1.1 client
//! so `Host` / `Origin` / `Authorization` can be controlled exactly (BIT-SP-0007.R1-R6).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use bitacora_mcp::{Error, McpConfig, McpServer, Scope, StaticGraphReader, TokenStore};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

struct Resp {
    status: u16,
    headers: String,
    body: String,
}

async fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Resp {
    let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str(&format!("Host: {addr}\r\n"));
    }
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut raw))
        .await
        .expect("response timeout")
        .expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status line");
    Resp {
        status,
        headers: head.to_ascii_lowercase(),
        body: body.to_owned(),
    }
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1"}}}"#;

fn json_headers(token: &str) -> Vec<(&str, &str)> {
    vec![
        ("Content-Type", "application/json"),
        ("Accept", "application/json, text/event-stream"),
        ("Authorization", token),
    ]
}

struct Fixture {
    server: McpServer,
    addr: SocketAddr,
    token: String,
}

/// Run blocking server lifecycle calls outside the test's async context (as a sync caller would).
fn off_runtime<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| s.spawn(f).join().expect("thread"))
}

fn start_on(
    config: McpConfig,
    reader: Arc<StaticGraphReader>,
    tokens: Arc<TokenStore>,
) -> Result<McpServer, Error> {
    off_runtime(move || McpServer::start(config, reader, tokens))
}

fn stop(server: McpServer) {
    off_runtime(move || server.stop());
}

fn start(config: McpConfig) -> Fixture {
    let tokens = Arc::new(TokenStore::in_memory());
    let token = tokens.create("test", &[Scope::Read]).expect("token");
    let reader = Arc::new(StaticGraphReader::new("demo", "/tmp/demo"));
    let server = start_on(McpConfig { port: 0, ..config }, reader, tokens).expect("start");
    let addr = server.local_addr();
    Fixture {
        server,
        addr,
        token: format!("Bearer {token}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn health_needs_no_token() {
    let f = start(McpConfig::default());
    let r = send(f.addr, "GET", "/health", &[], "").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.body.trim(), r#"{"ok":true}"#);
    assert!(!r.headers.contains("access-control-allow"));
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn initialize_handshake_with_token_stateful() {
    let f = start(McpConfig::default());
    let r = send(f.addr, "POST", "/mcp", &json_headers(&f.token), INIT).await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("serverInfo"), "{}", r.body);
    assert!(r.body.contains("bitacora"), "{}", r.body);
    assert!(
        r.headers.contains("mcp-session-id"),
        "stateful mode issues a session id"
    );
    assert!(!r.headers.contains("access-control-allow"));
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn stateless_json_mode_serves_tools() {
    let f = start(McpConfig {
        stateful: false,
        ..McpConfig::default()
    });
    let r = send(f.addr, "POST", "/mcp", &json_headers(&f.token), INIT).await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.headers.contains("application/json"), "{}", r.headers);
    assert!(r.body.contains("serverInfo"));

    let call = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_graph_info","arguments":{}}}"#;
    let r = send(f.addr, "POST", "/mcp", &json_headers(&f.token), call).await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("demo"), "{}", r.body);

    let ping =
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}"#;
    let r = send(f.addr, "POST", "/mcp", &json_headers(&f.token), ping).await;
    assert!(r.body.contains("pong"), "{}", r.body);
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_wrong_and_revoked_token_get_401() {
    let f = start(McpConfig::default());
    let none = send(
        f.addr,
        "POST",
        "/mcp",
        &[
            ("Content-Type", "application/json"),
            ("Accept", "application/json, text/event-stream"),
        ],
        INIT,
    )
    .await;
    assert_eq!(none.status, 401);
    assert!(none.headers.contains("www-authenticate: bearer"));

    for bad in [
        "Bearer nope",
        "Bearer ",
        "Basic abc",
        "bit_raw_without_scheme",
    ] {
        let r = send(f.addr, "POST", "/mcp", &json_headers(bad), INIT).await;
        assert_eq!(r.status, 401, "{bad}");
        assert!(r.headers.contains("www-authenticate: bearer"), "{bad}");
    }

    // Revocation takes effect on the running server.
    assert!(f.server.tokens().revoke("test").expect("revoke"));
    let r = send(f.addr, "POST", "/mcp", &json_headers(&f.token), INIT).await;
    assert_eq!(r.status, 401);
    // Empty token list refuses everything, including GET /mcp.
    let r = send(f.addr, "GET", "/mcp", &[("Authorization", &f.token)], "").await;
    assert_eq!(r.status, 401);
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn foreign_origin_and_host_get_403() {
    let f = start(McpConfig::default());
    let mut h = json_headers(&f.token);
    h.push(("Origin", "https://evil.example"));
    let r = send(f.addr, "POST", "/mcp", &h, INIT).await;
    assert_eq!(r.status, 403);
    assert!(!r.headers.contains("access-control-allow"));

    // Even a valid token cannot get past a `null` origin or a preflight.
    let mut h = json_headers(&f.token);
    h.push(("Origin", "null"));
    assert_eq!(send(f.addr, "POST", "/mcp", &h, INIT).await.status, 403);
    let r = send(
        f.addr,
        "OPTIONS",
        "/mcp",
        &[
            ("Origin", "https://evil.example"),
            ("Access-Control-Request-Method", "POST"),
        ],
        "",
    )
    .await;
    assert_eq!(r.status, 403);
    assert!(!r.headers.contains("access-control-allow"));
    // Foreign origin is refused on /health too.
    let r = send(
        f.addr,
        "GET",
        "/health",
        &[("Origin", "https://evil.example")],
        "",
    )
    .await;
    assert_eq!(r.status, 403);

    // DNS rebinding: attacker hostname resolving to 127.0.0.1.
    let mut h = json_headers(&f.token);
    h.push(("Host", "rebind.evil.example"));
    assert_eq!(send(f.addr, "POST", "/mcp", &h, INIT).await.status, 403);
    let mut h = json_headers(&f.token);
    h.push(("Host", "127.0.0.1:1"));
    assert_eq!(send(f.addr, "POST", "/mcp", &h, INIT).await.status, 403);
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn allowlisted_origin_is_accepted() {
    let f = start(McpConfig {
        allowed_origins: vec!["http://localhost:6274".to_owned()],
        ..McpConfig::default()
    });
    let mut h = json_headers(&f.token);
    h.push(("Origin", "http://localhost:6274"));
    let r = send(f.addr, "POST", "/mcp", &h, INIT).await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(
        !r.headers.contains("access-control-allow"),
        "no CORS headers even when allowed"
    );
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn non_loopback_bind_is_refused() {
    for ip in [
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        "192.168.1.10".parse().expect("ip"),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    ] {
        let err = start_on(
            McpConfig {
                bind: ip,
                port: 0,
                ..McpConfig::default()
            },
            Arc::new(StaticGraphReader::new("g", "/g")),
            Arc::new(TokenStore::in_memory()),
        )
        .expect_err("must refuse");
        assert!(matches!(err, Error::NonLoopbackBind(_)), "{err}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn busy_port_is_reported_without_fallback() {
    let f = start(McpConfig::default());
    let err = start_on(
        McpConfig {
            port: f.addr.port(),
            ..McpConfig::default()
        },
        Arc::new(StaticGraphReader::new("g", "/g")),
        Arc::new(TokenStore::in_memory()),
    )
    .expect_err("port busy");
    assert!(
        matches!(err, Error::PortInUse(p) if p == f.addr.port()),
        "{err}"
    );
    stop(f.server);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_releases_the_port() {
    let f = start(McpConfig::default());
    let addr = f.addr;
    stop(f.server);
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "server must not accept after stop"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ipv6_loopback_is_supported() {
    let tokens = Arc::new(TokenStore::in_memory());
    let server = match start_on(
        McpConfig {
            bind: IpAddr::V6(Ipv6Addr::LOCALHOST),
            port: 0,
            ..McpConfig::default()
        },
        Arc::new(StaticGraphReader::new("g", "/g")),
        tokens,
    ) {
        Ok(s) => s,
        Err(Error::Io(_)) => return, // host without IPv6
        Err(e) => panic!("{e}"),
    };
    let r = send(server.local_addr(), "GET", "/health", &[], "").await;
    assert_eq!(r.status, 200);
    stop(server);
}
