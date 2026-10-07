#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! The dedicated `pando` MCP token (ADR-031, BIT-US-0139): Read scope only, consent exclusions
//! enforced in every read path, writes rejected; other tokens are unaffected. A real axum server
//! on an ephemeral port over a real index of a small temp graph.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bitacora_index::{Index, IndexLocation, Indexer, IndexerOptions, OpenOptions};
use bitacora_mcp::{
    IndexGraphReader, McpConfig, McpServer, PANDO_TOKEN_NAME, ReadExclusions, Scope, TokenStore,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

const SECRET_BLOCK: &str = "33333333-3333-4333-8333-333333333333";

fn write(graph: &std::path::Path, rel: &str, text: &str) {
    let p = graph.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn off_runtime<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| s.spawn(f).join().expect("thread"))
}

struct Env {
    _tmp: tempfile::TempDir,
    indexer: Indexer,
    server: McpServer,
    addr: SocketAddr,
    pando: String,
    reader: String,
}

fn setup(agent_writes: bool) -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let graph = tmp.path().join("graph");
    std::fs::create_dir_all(&graph).unwrap();
    write(
        &graph,
        "pages/Public.md",
        "- zebra-marker public note\n- links nothing\n",
    );
    write(
        &graph,
        "pages/Secret.md",
        &format!("private:: true\n\n- zebra-marker secret [[Public]]\n  id:: {SECRET_BLOCK}\n"),
    );
    write(
        &graph,
        "pages/Diary.md",
        "- zebra-marker diary entry [[Public]]\n",
    );
    write(
        &graph,
        "pages/Plan.md",
        "tags:: confidential\n\n- zebra-marker plan [[Public]]\n",
    );
    write(
        &graph,
        "pages/Mentions.md",
        "- zebra-marker mentions [[Public]] openly\n",
    );
    let config = bitacora_config::EffectiveConfig::load(&graph, None);
    let loc = IndexLocation::in_data_dir(&tmp.path().join("data"), &graph).unwrap();
    let index = Index::open(loc, OpenOptions::for_config(&graph, &config)).unwrap();
    let indexer = Indexer::start(&index, IndexerOptions::new(&graph, config)).unwrap();
    indexer.reconcile().unwrap();
    let reader = IndexGraphReader::new(&index, &graph, "demo");

    let tokens = Arc::new(TokenStore::in_memory());
    let scopes: &[Scope] = if agent_writes {
        &[Scope::Read, Scope::Write]
    } else {
        &[Scope::Read]
    };
    let pando = tokens.ensure_token(PANDO_TOKEN_NAME, scopes).unwrap();
    let other = tokens.create("reader", &[Scope::Read]).unwrap();
    let cfg = McpConfig {
        port: 0,
        stateful: false,
        ..McpConfig::default()
    };
    let server = off_runtime(|| McpServer::start(cfg, Arc::new(reader), tokens)).unwrap();
    server.set_read_exclusions(
        PANDO_TOKEN_NAME,
        Some(ReadExclusions::new(["Diary", "#confidential"])),
    );
    let addr = server.local_addr();
    Env {
        _tmp: tmp,
        indexer,
        server,
        addr,
        pando: format!("Bearer {pando}"),
        reader: format!("Bearer {other}"),
    }
}

async fn rpc(env: &Env, token: &str, method: &str, params: Value) -> Value {
    let body = json!({"jsonrpc":"2.0","id":7,"method":method,"params":params}).to_string();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nAuthorization: {token}\r\n\
         Content-Length: {}\r\n\r\n{body}",
        env.addr,
        body.len()
    );
    let mut stream = TcpStream::connect(env.addr).await.unwrap();
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    assert!(head.contains(" 200 "), "{head}\n{body}");
    let json_body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut rest = body;
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            out.push_str(&tail[..n]);
            rest = tail[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        body.to_owned()
    };
    serde_json::from_str(json_body.trim()).unwrap_or_else(|_| panic!("not JSON: {json_body}"))
}

async fn call(env: &Env, token: &str, tool: &str, args: Value) -> Value {
    rpc(
        env,
        token,
        "tools/call",
        json!({"name": tool, "arguments": args}),
    )
    .await["result"]
        .clone()
}

fn stop(env: Env) {
    let Env {
        server, indexer, ..
    } = env;
    off_runtime(move || server.stop());
    indexer.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn excluded_pages_are_invisible_to_the_pando_token_everywhere() {
    let env = setup(false);

    // Pages: private property, excluded name and excluded tag all answer NOT_FOUND.
    for name in ["Secret", "Diary", "Plan"] {
        let r = call(&env, &env.pando, "get_page", json!({"name": name})).await;
        assert_eq!(r["isError"], true, "{name}: {r}");
        assert_eq!(r["structuredContent"]["code"], "NOT_FOUND", "{name}");
    }
    let r = call(&env, &env.pando, "get_page", json!({"name": "Public"})).await;
    assert_eq!(r["isError"], false, "{r}");

    // Search never returns excluded pages.
    let r = call(
        &env,
        &env.pando,
        "search",
        json!({"query": "zebra-marker", "limit": 20}),
    )
    .await;
    let hits = serde_json::to_string(&r["structuredContent"]["hits"]).unwrap();
    assert!(
        hits.contains("Public") && hits.contains("Mentions"),
        "{hits}"
    );
    for hidden in ["Secret", "Diary", "Plan"] {
        assert!(!hits.contains(hidden), "{hidden} leaked in {hits}");
    }

    // Backlinks of Public skip the excluded referrers.
    let r = call(&env, &env.pando, "backlinks", json!({"name": "Public"})).await;
    let refs = serde_json::to_string(&r["structuredContent"]).unwrap();
    assert!(refs.contains("Mentions"), "{refs}");
    for hidden in ["Secret", "Diary", "Plan"] {
        assert!(!refs.contains(hidden), "{hidden} leaked in {refs}");
    }

    // A block of an excluded page is not readable by uuid, a page listing omits the pages.
    let r = call(&env, &env.pando, "get_block", json!({"uuid": SECRET_BLOCK})).await;
    assert_eq!(r["isError"], true, "{r}");
    let r = call(&env, &env.pando, "list_pages", json!({"limit": 50})).await;
    let pages = serde_json::to_string(&r["structuredContent"]).unwrap();
    assert!(pages.contains("Public"), "{pages}");
    for hidden in ["Secret", "Diary", "Plan"] {
        assert!(!pages.contains(hidden), "{hidden} leaked in {pages}");
    }

    // Resources: the raw page of an excluded page is refused.
    let r = rpc(
        &env,
        &env.pando,
        "resources/read",
        json!({"uri": "bitacora://page/Secret"}),
    )
    .await;
    assert!(r.get("error").is_some(), "{r}");
    let r = rpc(
        &env,
        &env.pando,
        "resources/read",
        json!({"uri": "bitacora://page/Public"}),
    )
    .await;
    assert!(r.get("error").is_none(), "{r}");

    // The restriction is per token: another reader still sees the private page.
    let r = call(&env, &env.reader, "get_page", json!({"name": "Secret"})).await;
    assert_eq!(r["isError"], false, "{r}");
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pando_token_cannot_write() {
    let env = setup(false);
    for (tool, args) in [
        ("create_page", json!({"name": "Hacked", "content": "- x"})),
        (
            "append_block",
            json!({"page": "Public", "content": "injected"}),
        ),
    ] {
        let r = call(&env, &env.pando, tool, args).await;
        assert_eq!(r["isError"], true, "{tool}: {r}");
        let code = r["structuredContent"]["code"].as_str().unwrap_or("");
        assert!(
            matches!(code, "FORBIDDEN_SCOPE" | "READ_ONLY"),
            "{tool}: unexpected code {code}: {r}"
        );
    }
    stop(env);
}
