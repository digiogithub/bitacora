#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Live conformance against a real Pando. Every test is `#[ignore]`; run them with
//!
//! ```text
//! cargo test -p pando-rs --test live -- --ignored
//! ```
//!
//! AG-UI tests use `PANDO_LIVE_AGUI_URL` (+ optional `PANDO_LIVE_AGUI_TOKEN`) when set; otherwise
//! they spawn `pando agui-serve --no-tls --no-token` (binary from `PANDO_BIN`, default `pando` on
//! `PATH`) in a temp directory on a free port and kill it afterwards. No LLM call is made, so no
//! provider credentials are needed. REST tests need `PANDO_LIVE_REST_URL` (+ optional
//! `PANDO_LIVE_REST_TOKEN`) because `pando serve` is TLS-only with a self-signed certificate.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pando::agui::AguiOptions;
use pando::kb::SearchRequest;
use pando::{PandoClient, PandoConfig};

/// A spawned `pando agui-serve`, killed on drop.
struct Spawned {
    child: Child,
    url: String,
    _dir: std::path::PathBuf,
}

impl Drop for Spawned {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self._dir);
    }
}

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().expect("addr").port()
}

async fn wait_ready(url: &str) {
    let client = PandoClient::new(PandoConfig::new(url)).expect("client");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if client.agui().healthz().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("pando agui-serve did not become ready at {url}");
}

/// The AG-UI base URL, token and the guard keeping a spawned server alive.
async fn agui_target() -> (String, Option<String>, Option<Spawned>) {
    if let Ok(url) = std::env::var("PANDO_LIVE_AGUI_URL") {
        return (url, std::env::var("PANDO_LIVE_AGUI_TOKEN").ok(), None);
    }
    let bin = std::env::var("PANDO_BIN").unwrap_or_else(|_| "pando".to_owned());
    let dir = std::env::temp_dir().join(format!("pando-rs-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let port = free_port();
    let child = Command::new(bin)
        .args(["agui-serve", "--no-tls", "--no-token", "--port"])
        .arg(port.to_string())
        .arg("--cwd")
        .arg(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn `pando agui-serve` (set PANDO_BIN or PANDO_LIVE_AGUI_URL)");
    let url = format!("http://127.0.0.1:{port}");
    let guard = Spawned {
        child,
        url: url.clone(),
        _dir: dir,
    };
    wait_ready(&guard.url).await;
    (url, None, Some(guard))
}

fn agui_client(url: &str, token: Option<String>) -> pando::agui::AguiClient {
    let client = PandoClient::new(PandoConfig::new(url)).expect("client");
    let mut options = AguiOptions::default().with_base_url(url);
    if let Some(token) = token {
        options = options.with_token(token.as_str());
    }
    client.agui_with(options)
}

#[tokio::test]
#[ignore = "needs a real `pando` binary or PANDO_LIVE_AGUI_URL"]
async fn live_agui_info_and_health_decode() {
    let (url, token, _guard) = agui_target().await;
    let agui = agui_client(&url, token);

    let info = agui.info().await.unwrap();
    assert_eq!(info.protocol, "ag-ui");
    assert!(info.path.starts_with("/api/v1/agui"));
    assert!(info.capabilities.shared_state);

    let health = agui.healthz().await.unwrap();
    assert_eq!(health.status, "ok");
    assert!(health.uptime_seconds >= 0.0);
    assert!(!health.draining);
}

#[tokio::test]
#[ignore = "needs a real `pando` binary or PANDO_LIVE_AGUI_URL"]
async fn live_agui_thread_api_on_a_fresh_project() {
    let (url, token, _guard) = agui_target().await;
    let agui = agui_client(&url, token);

    let page = agui.list_threads(5, 0).await.unwrap();
    assert_eq!((page.limit, page.offset), (5, 0));
    assert!(page.threads.is_empty());
    assert!(!page.has_more);

    assert!(
        agui.thread_messages("no-such-thread")
            .await
            .unwrap()
            .is_none()
    );
    assert!(agui.attach("no-such-thread").await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs PANDO_LIVE_REST_URL pointing at a running `pando serve`"]
async fn live_rest_health_and_kb_search() {
    let Ok(url) = std::env::var("PANDO_LIVE_REST_URL") else {
        panic!("PANDO_LIVE_REST_URL is not set");
    };
    let mut config = PandoConfig::new(url);
    if let Ok(token) = std::env::var("PANDO_LIVE_REST_TOKEN") {
        config = config.with_token(token.as_str());
    }
    let client = PandoClient::new(config).unwrap();
    let info = client.info().await.unwrap();
    assert!(!info.version.is_empty());
    // A search either succeeds or reports the KB as not configured; both decode cleanly.
    match client
        .kb()
        .search(&SearchRequest::new("conformance", 3))
        .await
    {
        Ok(_) | Err(pando::Error::NotConfigured(_)) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}
