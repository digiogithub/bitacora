#![allow(clippy::expect_used, clippy::unwrap_used)]
//! End-to-end `serve`: a real index of a temp copy of the `logseq-docs` fixture behind the real
//! HTTP server, driven with a minimal blocking HTTP client (BIT-US-0017/0018).

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

use bitacora_mcp::{Scope, TokenStore};
use serde_json::{Value, json};

use super::serve::{ServeArgs, start};

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let dest = to.join(e.file_name());
        if e.file_type().expect("type").is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).expect("copy");
        }
    }
}

fn post(addr: &str, token: &str, session: Option<&str>, body: &Value) -> (String, String) {
    let body = body.to_string();
    let mut req = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nAuthorization: Bearer {token}\r\n"
    );
    if let Some(s) = session {
        req.push_str(&format!("Mcp-Session-Id: {s}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout");
    stream.write_all(req.as_bytes()).expect("write");
    let mut raw = String::new();
    let _ = stream.read_to_string(&mut raw);
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    (head.to_ascii_lowercase(), body.to_owned())
}

fn message(body: &str) -> Value {
    for line in body.lines() {
        let candidate = line.strip_prefix("data:").unwrap_or(line).trim();
        if let Ok(v) = serde_json::from_str::<Value>(candidate)
            && v.get("id").is_some()
        {
            return v;
        }
    }
    panic!("no JSON-RPC message in {body}");
}

#[test]
fn serve_answers_real_queries_over_http() {
    let tmp = tempfile::tempdir().expect("tmp");
    let graph = tmp.path().join("g");
    copy_dir(&bitacora_testkit::graph("logseq-docs"), &graph);
    let token_file = tmp.path().join("tokens.json");
    let store = TokenStore::load_or_init(&token_file).expect("tokens");
    let token = store.create("e2e", &[Scope::Read]).expect("token");
    drop(store);

    let running = start(ServeArgs {
        graph: graph.clone(),
        port: 0,
        token_file: Some(token_file),
        data_dir: Some(tmp.path().join("data")),
        allowed_origins: Vec::new(),
    })
    .expect("start serve");
    let addr = running
        .endpoint()
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .to_owned();

    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},
        "clientInfo":{"name":"e2e","version":"1"}}});
    let (head, body) = post(&addr, &token, None, &init);
    let session = head
        .lines()
        .find_map(|l| l.strip_prefix("mcp-session-id:"))
        .expect("session id")
        .trim()
        .to_owned();
    assert_eq!(message(&body)["result"]["serverInfo"]["name"], "bitacora");
    post(
        &addr,
        &token,
        Some(&session),
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    let rpc = |id: u64, method: &str, params: Value| {
        let (_, body) = post(
            &addr,
            &token,
            Some(&session),
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        );
        message(&body)
    };
    let tools = rpc(2, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        names.contains(&"get_page") && names.contains(&"search"),
        "{names:?}"
    );

    let page = rpc(
        3,
        "tools/call",
        json!({"name":"get_page","arguments":{"name":"aliases and external links"}}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["original_name"],
        "Aliases and external links"
    );
    let tree = rpc(
        4,
        "tools/call",
        json!({"name":"get_page_blocks_tree","arguments":{"name":"Aliases and external links"}}),
    );
    assert!(
        tree["result"]["structuredContent"]["markdown"]
            .as_str()
            .unwrap_or("")
            .contains("alias function"),
        "{tree}"
    );
    let found = rpc(
        5,
        "tools/call",
        json!({"name":"search","arguments":{"query":"alias"}}),
    );
    assert!(
        !found["result"]["structuredContent"]["hits"]
            .as_array()
            .expect("hits")
            .is_empty(),
        "{found}"
    );
    let info = rpc(
        6,
        "tools/call",
        json!({"name":"get_graph_info","arguments":{}}),
    );
    assert!(
        info["result"]["structuredContent"]["page_count"]
            .as_u64()
            .unwrap_or(0)
            > 100
    );
    running.stop();
}
