#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! `semantic_search` / `related_blocks` over a real axum server (BIT-US-0146): disabled errors and
//! the `pando` token's read exclusions applied to semantic candidates.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bitacora_index::{Index, IndexLocation, Indexer, IndexerOptions, OpenOptions};
use bitacora_mcp::{
    IndexGraphReader, McpConfig, McpServer, PANDO_TOKEN_NAME, ReadExclusions, Scope,
    SemanticFailure, SemanticMatch, SemanticProvider, TokenStore,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

const SECRET_BLOCK: &str = "33333333-3333-4333-8333-333333333333";
const PUBLIC_BLOCK: &str = "11111111-1111-4111-8111-111111111111";
const DIARY_BLOCK: &str = "22222222-2222-4222-8222-222222222222";

/// Answers every semantic query with the same candidates, hidden ones first.
struct Fake(Result<Vec<SemanticMatch>, SemanticFailure>);

impl SemanticProvider for Fake {
    fn search(&self, _q: &str, _limit: usize) -> Result<Vec<SemanticMatch>, SemanticFailure> {
        self.0.clone()
    }
}

fn candidates() -> Vec<SemanticMatch> {
    [SECRET_BLOCK, DIARY_BLOCK, PUBLIC_BLOCK]
        .iter()
        .enumerate()
        .map(|(i, u)| SemanticMatch {
            uuid: (*u).to_owned(),
            score: 1.0 / (i as f64 + 1.0),
            stale: false,
        })
        .collect()
}

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
        &format!("- zebra-marker public note\n  id:: {PUBLIC_BLOCK}\n- links nothing\n"),
    );
    write(
        &graph,
        "pages/Secret.md",
        &format!("private:: true\n\n- zebra-marker secret [[Public]]\n  id:: {SECRET_BLOCK}\n"),
    );
    write(
        &graph,
        "pages/Diary.md",
        &format!("- zebra-marker diary entry [[Public]]\n  id:: {DIARY_BLOCK}\n"),
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
async fn disabled_graph_answers_semantic_disabled() {
    let env = setup(false);
    for (tool, args) in [
        ("semantic_search", json!({"query": "zebra"})),
        ("related_blocks", json!({"block_uuid": PUBLIC_BLOCK})),
    ] {
        let r = call(&env, &env.reader, tool, args.clone()).await;
        assert_eq!(r["isError"], true, "{tool}: {r}");
        assert_eq!(
            r["structuredContent"]["code"], "SEMANTIC_DISABLED",
            "{tool}"
        );
    }
    env.server
        .set_semantic_provider(Some(Arc::new(Fake(Err(SemanticFailure::Disabled)))));
    let r = call(
        &env,
        &env.reader,
        "semantic_search",
        json!({"query": "zebra"}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "SEMANTIC_DISABLED", "{r}");
    env.server
        .set_semantic_provider(Some(Arc::new(Fake(Err(SemanticFailure::Unavailable(
            "offline".into(),
        ))))));
    let r = call(
        &env,
        &env.reader,
        "semantic_search",
        json!({"query": "zebra"}),
    )
    .await;
    assert_eq!(
        r["structuredContent"]["code"], "SEMANTIC_UNAVAILABLE",
        "{r}"
    );
    let r = call(&env, &env.reader, "semantic_search", json!({"query": "  "})).await;
    assert_eq!(r["structuredContent"]["code"], "INVALID_ARGUMENT", "{r}");
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn semantic_results_respect_the_tokens_read_exclusions() {
    let env = setup(false);
    env.server
        .set_semantic_provider(Some(Arc::new(Fake(Ok(candidates())))));

    // The pando token never sees blocks of excluded or private pages.
    let r = call(
        &env,
        &env.pando,
        "semantic_search",
        json!({"query": "zebra"}),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    let hits = r["structuredContent"]["hits"].as_array().unwrap().clone();
    let all = serde_json::to_string(&hits).unwrap();
    assert_eq!(hits.len(), 1, "{all}");
    assert_eq!(hits[0]["uuid"], PUBLIC_BLOCK);
    assert!(!all.contains("secret") && !all.contains("diary"), "{all}");
    let text = r["content"][0]["text"].as_str().unwrap();
    assert!(
        !text.contains("secret") && !text.contains("diary"),
        "{text}"
    );

    // related_blocks: the source block of an excluded page is not found for that token.
    let r = call(
        &env,
        &env.pando,
        "related_blocks",
        json!({"block_uuid": SECRET_BLOCK}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "NOT_FOUND", "{r}");

    // An unrestricted token sees everything the provider returned; the source is skipped.
    let r = call(
        &env,
        &env.reader,
        "semantic_search",
        json!({"query": "zebra"}),
    )
    .await;
    assert_eq!(
        r["structuredContent"]["hits"].as_array().unwrap().len(),
        3,
        "{r}"
    );
    let r = call(
        &env,
        &env.reader,
        "related_blocks",
        json!({"block_uuid": PUBLIC_BLOCK}),
    )
    .await;
    let uuids: Vec<String> = r["structuredContent"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["uuid"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(uuids, [SECRET_BLOCK, DIARY_BLOCK], "{r}");
    stop(env);
}
