//! Semantic sync wired into a session (BIT-US-0143): consent gates it, consenting graphs are sent
//! to Pando and the pending work is kept for the next session.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bitacora_config::{PandoMode, PandoSettings};
use bitacora_pando::PandoCredentials;
use bitacora_runtime::{PandoOptions, Session};

const PAGE: &str = "- A block that is long enough to be worth indexing\n\
- Another block that is also long enough to be indexed\n";

/// A tiny HTTP server: `/health` and the KB document routes, one request per connection.
fn mock_pando() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head, body_start) = loop {
                    let Ok(n) = s.read(&mut chunk) else { return };
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break (String::from_utf8_lossy(&buf[..i]).into_owned(), i + 4);
                    }
                };
                let len = head
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                while buf.len() < body_start + len {
                    let Ok(n) = s.read(&mut chunk) else { return };
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let request_line = head.lines().next().unwrap_or_default().to_owned();
                let body = String::from_utf8_lossy(&buf[body_start..]).into_owned();
                let reply = if request_line.contains("/health") {
                    r#"{"version":"test"}"#.to_owned()
                } else {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    log.lock().unwrap().push(format!(
                        "{} {}",
                        request_line.split(' ').next().unwrap_or(""),
                        v["file_path"].as_str().unwrap_or("")
                    ));
                    format!(
                        r#"{{"file_path":{},"action":"created","status":"deleted"}}"#,
                        v["file_path"]
                    )
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.len(),
                    reply
                );
            });
        }
    });
    (url, seen)
}

fn opts(graph: &Path, rest: &str, consent: bool) -> PandoOptions {
    let canonical = std::fs::canonicalize(graph).unwrap();
    let mut settings = PandoSettings {
        enabled: true,
        mode: PandoMode::External,
        rest_url: rest.to_owned(),
        agui_url: rest.to_owned(),
        ..PandoSettings::default()
    };
    if consent {
        settings.grant_consent(&canonical.to_string_lossy(), 1);
    }
    let mut o = PandoOptions::new(settings, canonical);
    o.credentials = PandoCredentials::new(None, |_| None);
    o.probe_interval = Duration::from_millis(100);
    o
}

#[test]
fn consenting_graph_is_indexed_semantically_and_restart_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let graph = common::graph_with(dir.path(), &[("pages/a.md", PAGE)]);
    let data = dir.path().join("data");
    let (url, seen) = mock_pando();

    let mut cfg = common::config(&graph, &data);
    cfg.pando = Some(opts(&graph, &url, true));
    let s = Session::open(cfg).unwrap();
    common::wait_for("both blocks sent", Duration::from_secs(20), || {
        (seen.lock().unwrap().len() >= 2).then_some(())
    });
    common::wait_for("outbox drained", Duration::from_secs(20), || {
        s.semantic_status()
            .filter(|st| st.synced == 2 && st.pending == 0)
    });
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|l| l.starts_with("POST bitacora/"))
    );
    s.shutdown(Duration::from_secs(5));

    // Second session over the same data directory: the ledger knows everything.
    let before = seen.lock().unwrap().len();
    let mut cfg = common::config(&graph, &data);
    cfg.pando = Some(opts(&graph, &url, true));
    let s = Session::open(cfg).unwrap();
    common::wait_for("connected", Duration::from_secs(20), || {
        s.semantic_status().filter(|st| st.synced == 2)
    });
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(seen.lock().unwrap().len(), before, "nothing re-sent");
    s.shutdown(Duration::from_secs(5));
}

#[test]
fn without_consent_nothing_is_read_or_sent() {
    let dir = tempfile::tempdir().unwrap();
    let graph = common::graph_with(dir.path(), &[("pages/a.md", PAGE)]);
    let (url, seen) = mock_pando();
    let mut cfg = common::config(&graph, &dir.path().join("data"));
    cfg.pando = Some(opts(&graph, &url, false));
    let s = Session::open(cfg).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(s.semantic_status().is_none());
    assert!(seen.lock().unwrap().is_empty());
    s.shutdown(Duration::from_secs(5));
}
