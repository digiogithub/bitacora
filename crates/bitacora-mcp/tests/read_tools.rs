#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! End-to-end read tools, resources and prompts: a real axum server on an ephemeral port over a
//! real index of a temp copy of the `logseq-docs` fixture plus purpose-made pages
//! (BIT-SP-0007.R11, R15-R19).

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bitacora_index::{Index, IndexLocation, Indexer, IndexerOptions, OpenOptions};
use bitacora_mcp::{
    IndexGraphReader, McpConfig, McpServer, Scope, SyncState, SyncStatus, SyncStatusProvider,
    TokenStore,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

const PX_ID: &str = "11111111-1111-4111-8111-111111111111";
const CHILD_ID: &str = "22222222-2222-4222-8222-222222222222";

fn copy_dir(from: &Path, to: &Path) {
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

fn write(graph: &Path, rel: &str, text: &str) {
    let p = graph.join(rel);
    std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    std::fs::write(p, text).expect("write");
}

fn today_file() -> (String, String) {
    let d = jiff::Zoned::now().date();
    (
        format!(
            "journals/{:04}_{:02}_{:02}.md",
            d.year(),
            d.month(),
            d.day()
        ),
        format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()),
    )
}

struct Env {
    _tmp: tempfile::TempDir,
    graph: std::path::PathBuf,
    indexer: Indexer,
    server: McpServer,
    addr: SocketAddr,
    token: String,
    write_token: String,
}

struct FakeSync;
impl SyncStatusProvider for FakeSync {
    fn status(&self) -> SyncStatus {
        SyncStatus {
            state: SyncState::Conflicted,
            conflict_count: 1,
            conflict_pages: vec!["Project X".into()],
            ..SyncStatus::default()
        }
    }
}

fn off_runtime<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| s.spawn(f).join().expect("thread"))
}

fn setup(stateful: bool, with_sync: bool) -> Env {
    let tmp = tempfile::tempdir().expect("tmp");
    let graph = tmp.path().join("graph");
    copy_dir(&bitacora_testkit::graph("logseq-docs"), &graph);
    write(
        &graph,
        "pages/Project X.md",
        &format!(
            "alias:: PX, Projx\ntags:: work\n\n\
             - TODO [#A] Write spec\n  SCHEDULED: <2020-01-01 Wed>\n  id:: {PX_ID}\n\
             \t- child one\n\t  id:: {CHILD_ID}\n\t\t- grandchild\n\t- child two\n\
             - DONE Ship it\n\
             - collapsed parent\n  collapsed:: true\n\t- hidden child\n"
        ),
    );
    write(
        &graph,
        "pages/Notes.md",
        &format!(
            "- Links to [[Project X]] here\n- See (({PX_ID}))\n- a plain mention of Project X text\n"
        ),
    );
    let (today_path, _) = today_file();
    write(
        &graph,
        &today_path,
        "- TODO journal task\n- Met about [[Project X]]\n",
    );
    write(&graph, "journals/2020_01_01.md", "- old day\n");
    write(&graph, "assets/pic.png", "fakepng");

    let config = bitacora_config::EffectiveConfig::load(&graph, None);
    let data = tmp.path().join("data");
    let loc = IndexLocation::in_data_dir(&data, &graph).expect("loc");
    let index = Index::open(loc, OpenOptions::for_config(&graph, &config)).expect("index");
    let indexer = Indexer::start(&index, IndexerOptions::new(&graph, config)).expect("indexer");
    let events = indexer.subscribe();
    indexer.reconcile().expect("reconcile");
    let reader = IndexGraphReader::new(&index, &graph, "demo-graph");
    reader.forward_events(events);

    let tokens = Arc::new(TokenStore::in_memory());
    let token = tokens.create("reader", &[Scope::Read]).expect("token");
    let write_token = tokens.create("writer", &[Scope::Write]).expect("token");
    let cfg = McpConfig {
        port: 0,
        stateful,
        ..McpConfig::default()
    };
    let server = if with_sync {
        off_runtime(|| {
            McpServer::start_with_sync(cfg, Arc::new(reader), Arc::new(FakeSync), tokens)
        })
    } else {
        off_runtime(|| McpServer::start(cfg, Arc::new(reader), tokens))
    }
    .expect("start");
    let addr = server.local_addr();
    Env {
        _tmp: tmp,
        graph,
        indexer,
        server,
        addr,
        token: format!("Bearer {token}"),
        write_token: format!("Bearer {write_token}"),
    }
}

fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, tail)) = rest.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else {
            return body.to_owned();
        };
        if n == 0 {
            break;
        }
        out.push_str(&tail[..n]);
        rest = tail[n..].trim_start_matches("\r\n");
    }
    out
}

async fn post(
    env: &Env,
    token: &str,
    session: Option<&str>,
    body: &Value,
) -> (u16, String, String) {
    let body = body.to_string();
    let mut req = format!(
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nAuthorization: {token}\r\n",
        env.addr
    );
    if let Some(s) = session {
        req.push_str(&format!("Mcp-Session-Id: {s}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    let mut stream = TcpStream::connect(env.addr).await.expect("connect");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut raw))
        .await
        .expect("timeout")
        .expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let head = head.to_ascii_lowercase();
    let body = if head.contains("transfer-encoding: chunked") {
        dechunk(body)
    } else {
        body.to_owned()
    };
    (status, head, body)
}

/// Parse a JSON or SSE (`data:` lines) body into the JSON-RPC message with an `id`.
fn rpc_message(body: &str) -> Value {
    if let Ok(v) = serde_json::from_str::<Value>(body.trim()) {
        return v;
    }
    for line in body.lines() {
        if let Some(d) = line.strip_prefix("data:")
            && let Ok(v) = serde_json::from_str::<Value>(d.trim())
            && v.get("id").is_some()
        {
            return v;
        }
    }
    panic!("no JSON-RPC message in {body}");
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1"}}}"#;

/// Stateless JSON mode: one POST, one JSON answer.
async fn rpc(env: &Env, token: &str, method: &str, params: Value) -> Value {
    let (status, _, body) = post(
        env,
        token,
        None,
        &json!({"jsonrpc":"2.0","id":7,"method":method,"params":params}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    rpc_message(&body)
}

async fn call(env: &Env, tool: &str, args: Value) -> Value {
    let r = rpc(
        env,
        &env.token,
        "tools/call",
        json!({"name":tool,"arguments":args}),
    )
    .await;
    r["result"].clone()
}

fn stop(env: Env) {
    let Env {
        server, indexer, ..
    } = env;
    off_runtime(move || server.stop());
    indexer.shutdown();
}

fn text_of(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_list_declares_schemas_and_read_only_hints() {
    let env = setup(false, false);
    let r = rpc(&env, &env.token, "tools/list", json!({})).await;
    let tools = r["result"]["tools"].as_array().expect("tools").clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "search",
        "get_page",
        "list_pages",
        "get_page_blocks_tree",
        "get_block_tree",
        "get_block",
        "list_journals",
        "get_today_journal",
        "backlinks",
        "tasks",
        "query",
        "get_graph_info",
        "list_graphs",
        "git_sync_status",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }
    for t in &tools {
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{}", t["name"]);
        assert!(t["inputSchema"].is_object(), "{}", t["name"]);
        if t["name"] != "ping" {
            assert!(t["outputSchema"].is_object(), "{}", t["name"]);
            assert!(!t["description"].as_str().unwrap_or("").is_empty());
        }
    }
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn search_get_page_and_list_pages() {
    let env = setup(false, false);
    let r = call(
        &env,
        "search",
        json!({"query": "aliases external", "limit": 5}),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    let hits = r["structuredContent"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "{r}");
    assert!(hits.len() <= 5);
    assert!(text_of(&r).contains("Search results"));

    // Case-insensitive name, and alias resolution.
    for name in ["project x", "PX", "projx"] {
        let r = call(&env, "get_page", json!({"name": name})).await;
        assert_eq!(r["isError"], false, "{name}: {r}");
        let p = &r["structuredContent"];
        assert_eq!(p["original_name"], "Project X", "{name}");
        assert_eq!(p["block_count"], 7, "{p}");
        assert_eq!(p["aliases"], json!(["PX", "Projx"]));
        assert_eq!(p["tags"], json!(["work"]));
        assert_eq!(p["file"], "pages/Project X.md");
        assert_eq!(p["etag"].as_str().unwrap().len(), 16);
    }
    let r = call(&env, "get_page", json!({"name": "no such page"})).await;
    assert_eq!(r["isError"], true);
    assert_eq!(r["structuredContent"]["code"], "NOT_FOUND");
    let r = call(
        &env,
        "get_page",
        json!({"name": "Project X", "graph": "other"}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "NOT_FOUND");
    let r = call(
        &env,
        "get_page",
        json!({"name": "Project X", "graph": "DEMO-graph"}),
    )
    .await;
    assert_eq!(r["isError"], false);

    // Pagination: pages are sorted by name; walk with a small page size.
    let mut seen = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..400 {
        let r = call(&env, "list_pages", json!({"limit": 50, "cursor": cursor})).await;
        let out = &r["structuredContent"];
        for p in out["pages"].as_array().unwrap() {
            seen.push(p["name"].as_str().unwrap().to_owned());
        }
        if out["next_cursor"].is_null() {
            break;
        }
        cursor = out["next_cursor"].clone();
    }
    assert!(seen.contains(&"project x".to_owned()));
    assert!(seen.len() > 100, "{}", seen.len());
    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), seen.len(), "no duplicates across pages");

    let r = call(&env, "list_pages", json!({"tag": "work"})).await;
    let names: Vec<&str> = r["structuredContent"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["project x"]);
    let r = call(&env, "list_pages", json!({"namespace": "nothing"})).await;
    assert!(
        r["structuredContent"]["pages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn block_trees_and_get_block() {
    let env = setup(false, false);
    let r = call(&env, "get_page_blocks_tree", json!({"name": "Project X"})).await;
    assert_eq!(r["isError"], false, "{r}");
    let out = &r["structuredContent"];
    let roots = out["blocks"].as_array().unwrap();
    assert_eq!(roots.len(), 3, "{out}");
    assert_eq!(roots[0]["uuid"], PX_ID);
    assert_eq!(roots[0]["marker"], "TODO");
    assert_eq!(roots[0]["priority"], "A");
    assert_eq!(roots[0]["scheduled"], "2020-01-01");
    assert!(!roots[0]["version"].as_str().unwrap().is_empty());
    assert_eq!(roots[0]["children"][0]["uuid"], CHILD_ID);
    assert_eq!(
        roots[0]["children"][0]["children"][0]["content"],
        "grandchild"
    );
    let md = out["markdown"].as_str().unwrap();
    assert!(md.contains("- TODO [#A] Write spec"), "{md}");
    assert!(md.contains("\t- child one"), "{md}");
    assert!(md.contains("\t\t- grandchild"), "{md}");
    assert!(out["etag"].is_string());
    assert_eq!(out["truncated"], false);

    // Depth and collapsed handling.
    let r = call(
        &env,
        "get_page_blocks_tree",
        json!({"name": "Project X", "max_depth": 1}),
    )
    .await;
    assert!(
        r["structuredContent"]["blocks"][0]
            .get("children")
            .is_none()
    );
    let r = call(
        &env,
        "get_page_blocks_tree",
        json!({"name": "Project X", "collapsed_children": false}),
    )
    .await;
    let md = r["structuredContent"]["markdown"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        md.contains("collapsed parent") && !md.contains("hidden child"),
        "{md}"
    );
    let r = call(
        &env,
        "get_page_blocks_tree",
        json!({"name": "Project X", "include_properties": false}),
    )
    .await;
    let md = r["structuredContent"]["markdown"].as_str().unwrap();
    assert!(!md.contains("id::") && !md.contains("collapsed::"), "{md}");

    // Pagination by block count.
    let mut total = 0;
    let mut cursor = Value::Null;
    for _ in 0..20 {
        let r = call(
            &env,
            "get_page_blocks_tree",
            json!({"name": "Project X", "limit": 2, "cursor": cursor}),
        )
        .await;
        let out = &r["structuredContent"];
        fn count(v: &Value) -> usize {
            v.as_array()
                .map_or(0, |a| a.iter().map(|b| 1 + count(&b["children"])).sum())
        }
        total += count(&out["blocks"]);
        if out["next_cursor"].is_null() {
            break;
        }
        assert_eq!(out["truncated"], true);
        cursor = out["next_cursor"].clone();
    }
    assert_eq!(total, 7);

    // uuid mode and the alias tool.
    let r = call(&env, "get_page_blocks_tree", json!({"uuid": PX_ID})).await;
    assert_eq!(r["structuredContent"]["root"], PX_ID);
    let r = call(&env, "get_block_tree", json!({"uuid": CHILD_ID})).await;
    assert_eq!(
        r["structuredContent"]["blocks"][0]["content"]
            .as_str()
            .unwrap()
            .lines()
            .next(),
        Some("child one")
    );
    let r = call(
        &env,
        "get_page_blocks_tree",
        json!({"name": "x", "uuid": "y"}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "INVALID_ARGUMENT");

    let r = call(
        &env,
        "get_block",
        json!({"uuid": CHILD_ID, "include_parents": true, "include_children": true}),
    )
    .await;
    let out = &r["structuredContent"];
    assert_eq!(out["block"]["uuid"], CHILD_ID);
    assert_eq!(out["block"]["page"], "Project X");
    assert_eq!(out["block"]["children"][0]["content"], "grandchild");
    assert_eq!(out["parents"][0]["uuid"], PX_ID);
    let r = call(
        &env,
        "get_block",
        json!({"uuid": "00000000-0000-4000-8000-000000000000"}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "NOT_FOUND");
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn journals_today_backlinks_tasks_query_graph_and_sync() {
    let env = setup(false, false);
    let (_, today_iso) = today_file();
    let r = call(&env, "list_journals", json!({})).await;
    let js = r["structuredContent"]["journals"]
        .as_array()
        .unwrap()
        .clone();
    assert!(js.len() <= 7);
    assert_eq!(js[0]["journal_day"], today_iso.as_str());
    let r = call(
        &env,
        "list_journals",
        json!({"from": "2020-01-01", "to": "2020-01-01"}),
    )
    .await;
    let js = r["structuredContent"]["journals"].as_array().unwrap();
    assert_eq!(js.len(), 1);
    assert_eq!(js[0]["journal_day"], "2020-01-01");
    let r = call(&env, "list_journals", json!({"limit": 1})).await;
    assert!(r["structuredContent"]["next_cursor"].is_string());

    let r = call(&env, "get_today_journal", json!({})).await;
    let out = &r["structuredContent"];
    assert_eq!(out["exists"], true, "{r}");
    assert_eq!(out["date"], today_iso.as_str());
    assert!(
        out["markdown"]
            .as_str()
            .unwrap()
            .contains("- TODO journal task")
    );
    let r = call(
        &env,
        "get_today_journal",
        json!({"create_if_missing": true}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "READ_ONLY");

    let r = call(
        &env,
        "backlinks",
        json!({"name": "PX", "include_unlinked": true}),
    )
    .await;
    let out = &r["structuredContent"];
    let pages: Vec<&str> = out["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["page"].as_str().unwrap())
        .collect();
    assert!(
        pages.contains(&"Notes") && pages.iter().any(|p| p.contains('_') || p.contains("20")),
        "{pages:?}"
    );
    assert!(
        out["unlinked"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["page"] == "Notes"),
        "{out}"
    );
    let r = call(&env, "backlinks", json!({"uuid": PX_ID})).await;
    let g = &r["structuredContent"]["groups"][0];
    assert_eq!(g["page"], "Notes");
    assert!(
        g["blocks"][0]["block"]["content"]
            .as_str()
            .unwrap()
            .contains(PX_ID)
    );
    let r = call(&env, "backlinks", json!({})).await;
    assert_eq!(r["structuredContent"]["code"], "INVALID_ARGUMENT");

    let r = call(
        &env,
        "tasks",
        json!({"status": ["TODO"], "page": "Project X"}),
    )
    .await;
    let tasks = r["structuredContent"]["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["uuid"], PX_ID);
    let r = call(&env, "tasks", json!({"scheduled_before": "2020-01-02"})).await;
    assert_eq!(r["structuredContent"]["tasks"].as_array().unwrap().len(), 1);
    let r = call(&env, "tasks", json!({"scheduled_before": "2019-12-31"})).await;
    assert!(
        r["structuredContent"]["tasks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let r = call(
        &env,
        "tasks",
        json!({"priority": "A", "status": ["done", "todo"]}),
    )
    .await;
    assert_eq!(r["structuredContent"]["tasks"].as_array().unwrap().len(), 1);
    let r = call(&env, "tasks", json!({"priority": "Z"})).await;
    assert_eq!(r["structuredContent"]["code"], "INVALID_ARGUMENT");
    let r = call(&env, "tasks", json!({"status": ["DONE"], "limit": 1})).await;
    assert_eq!(r["structuredContent"]["tasks"][0]["marker"], "DONE");

    let r = call(
        &env,
        "query",
        json!({"dsl": "(and (task TODO) [[Project X]])"}),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    // Page references include the owning page's path, so tasks on `Project X` match.
    assert!(
        !r["structuredContent"]["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let r = call(&env, "query", json!({"dsl": "(task TODO DOING)"})).await;
    assert!(r["structuredContent"]["blocks"].as_array().unwrap().len() >= 2);
    let r = call(
        &env,
        "query",
        json!({"dsl": "[:find ?b :where [?b :block/marker"}),
    )
    .await;
    assert_eq!(r["structuredContent"]["code"], "INVALID_QUERY");
    let r = call(&env, "query", json!({"dsl": "(or (task TODO))"})).await;
    assert_eq!(r["isError"], false, "{r}");

    let r = call(&env, "get_graph_info", json!({})).await;
    assert_eq!(r["structuredContent"]["name"], "demo-graph");
    assert!(r["structuredContent"]["page_count"].as_u64().unwrap() > 100);
    let r = call(&env, "list_graphs", json!({})).await;
    assert_eq!(
        r["structuredContent"]["graphs"].as_array().unwrap().len(),
        1
    );
    let r = call(&env, "git_sync_status", json!({})).await;
    assert_eq!(r["structuredContent"]["state"], "disabled");
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_status_comes_from_the_provider() {
    let env = setup(false, true);
    let r = call(&env, "git_sync_status", json!({})).await;
    assert_eq!(r["structuredContent"]["state"], "conflicted");
    assert_eq!(
        r["structuredContent"]["conflict_pages"],
        json!(["Project X"])
    );
    let r = rpc(
        &env,
        &env.token,
        "resources/read",
        json!({"uri": "bitacora://sync/status"}),
    )
    .await;
    assert!(
        r["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("conflicted")
    );
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_enforce_the_read_scope() {
    let env = setup(false, false);
    let r = rpc(
        &env,
        &env.write_token,
        "tools/call",
        json!({"name": "get_page", "arguments": {"name": "Project X"}}),
    )
    .await;
    let res = &r["result"];
    assert_eq!(res["isError"], true, "{r}");
    assert_eq!(res["structuredContent"]["code"], "FORBIDDEN_SCOPE");
    assert!(!text_of(res).contains("Project X\n"));
    let r = rpc(&env, &env.write_token, "resources/list", json!({})).await;
    assert!(r.get("error").is_some(), "{r}");
    let r = rpc(&env, &env.write_token, "prompts/list", json!({})).await;
    assert!(r.get("error").is_some(), "{r}");
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn resources_templates_read_and_confinement() {
    let env = setup(false, false);
    let r = rpc(&env, &env.token, "resources/templates/list", json!({})).await;
    let templates: Vec<String> = r["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap().to_owned())
        .collect();
    for want in [
        "bitacora://page/{name}",
        "bitacora://graph/{graph}/page/{name}",
        "bitacora://block/{uuid}",
        "bitacora://journal/{date}",
        "bitacora://graph/{graph}/config",
        "bitacora://sync/status",
        "bitacora://asset/{path}",
    ] {
        assert!(templates.iter().any(|t| t == want), "{want}: {templates:?}");
    }

    let r = rpc(&env, &env.token, "resources/list", json!({})).await;
    let list = r["result"]["resources"].as_array().unwrap();
    assert!(list.iter().any(|x| x["uri"] == "bitacora://journal/today"));
    assert!(
        list.iter()
            .any(|x| x["uri"] == "bitacora://page/Project%20X"),
        "{list:?}"
    );

    let read = |uri: &str| {
        let uri = uri.to_owned();
        let env = &env;
        async move { rpc(env, &env.token, "resources/read", json!({ "uri": uri })).await }
    };
    // Raw file text, byte for byte.
    let r = read("bitacora://page/project%20x").await;
    let c = &r["result"]["contents"][0];
    assert_eq!(c["mimeType"], "text/markdown");
    let on_disk = std::fs::read_to_string(env.graph.join("pages/Project X.md")).unwrap();
    assert_eq!(c["text"].as_str().unwrap(), on_disk);
    let r = read("bitacora://graph/demo-graph/page/PX").await;
    assert_eq!(
        r["result"]["contents"][0]["text"].as_str().unwrap(),
        on_disk
    );
    let r = read(&format!("bitacora://block/{CHILD_ID}")).await;
    let t = r["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        t.starts_with("- child one") && t.contains("\t- grandchild"),
        "{t}"
    );
    let (_, today_iso) = today_file();
    let r = read("bitacora://journal/today").await;
    assert!(
        r["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("journal task")
    );
    let r = read(&format!("bitacora://journal/{today_iso}")).await;
    assert!(r["result"]["contents"][0]["text"].is_string());
    let r = read("bitacora://graph/demo-graph/config").await;
    assert!(r["result"]["contents"][0]["text"].is_string(), "{r}");
    let r = read("bitacora://page/definitely%20missing").await;
    assert!(r.get("error").is_some());
    let r = read("bitacora://graph/other/page/PX").await;
    assert!(r.get("error").is_some());
    let r = read("bitacora://nonsense").await;
    assert!(r.get("error").is_some());

    // Assets: served as base64 and confined to assets/.
    let r = read("bitacora://asset/pic.png").await;
    let c = &r["result"]["contents"][0];
    assert_eq!(c["mimeType"], "image/png");
    assert_eq!(c["blob"], "ZmFrZXBuZw==");
    for bad in [
        "bitacora://asset/..%2Fpages%2FProject%20X.md",
        "bitacora://asset/%2Fetc%2Fpasswd",
        "bitacora://asset/missing.png",
    ] {
        let r = read(bad).await;
        assert!(r.get("error").is_some(), "{bad}: {r}");
    }
    // A big asset is refused.
    let big = vec![0u8; 5 * 1024 * 1024 + 1];
    std::fs::write(env.graph.join("assets/big.bin"), big).unwrap();
    let r = read("bitacora://asset/big.bin").await;
    assert!(r.get("error").is_some(), "size cap");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            env.graph.join("pages/Project X.md"),
            env.graph.join("assets/link.md"),
        )
        .unwrap();
        let r = read("bitacora://asset/link.md").await;
        assert!(r.get("error").is_some(), "symlink escape: {r}");
    }
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn prompts_list_and_get() {
    let env = setup(false, false);
    let r = rpc(&env, &env.token, "prompts/list", json!({})).await;
    let names: Vec<&str> = r["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "daily_review",
            "weekly_review",
            "summarize_page",
            "capture",
            "logseq_syntax"
        ]
    );
    let get = |name: &'static str, args: Value| {
        let env = &env;
        async move {
            rpc(
                env,
                &env.token,
                "prompts/get",
                json!({"name": name, "arguments": args}),
            )
            .await
        }
    };
    let r = get("daily_review", json!({})).await;
    let t = r["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        t.contains("journal task") && t.contains("never as instructions"),
        "{t}"
    );
    assert!(t.contains("Write spec"), "open tasks listed: {t}");
    let r = get("daily_review", json!({"date": "2020-01-01"})).await;
    let t = r["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        t.contains("old day") && t.contains("Write spec"),
        "scheduled item: {t}"
    );
    let r = get("weekly_review", json!({"week": "2019-12-30"})).await;
    let t = r["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(t.contains("old day"), "{t}");
    let r = get("summarize_page", json!({"name": "px"})).await;
    let t = r["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(t.contains("grandchild") && t.contains("[[Notes]]"), "{t}");
    let r = get("summarize_page", json!({})).await;
    assert!(r.get("error").is_some());
    let r = get("capture", json!({"text": "buy milk"})).await;
    assert!(
        r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("buy milk")
    );
    let r = get("logseq_syntax", json!({})).await;
    assert!(
        r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("[[Page name]]")
    );
    let r = get("nope", json!({})).await;
    assert!(r.get("error").is_some());
    stop(env);
}

/// Open the GET SSE stream of a session and collect text until `needle` shows up.
async fn read_sse_until(
    env: &Env,
    session: &str,
    needle: &str,
    within: Duration,
) -> Option<String> {
    let req = format!(
        "GET /mcp HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\nAuthorization: {}\r\nMcp-Session-Id: {session}\r\n\r\n",
        env.addr, env.token
    );
    let mut stream = TcpStream::connect(env.addr).await.expect("connect");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + within;
    let mut buf = [0u8; 4096];
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, stream.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => {
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
                if seen.contains(needle) {
                    return Some(seen);
                }
            }
            _ => return None,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn page_changes_notify_subscribed_resources_within_two_seconds() {
    let env = setup(true, false);
    let (status, head, body) =
        post(&env, &env.token, None, &serde_json::from_str(INIT).unwrap()).await;
    assert_eq!(status, 200, "{body}");
    let session = head
        .lines()
        .find_map(|l| l.strip_prefix("mcp-session-id:"))
        .expect("session id")
        .trim()
        .to_owned();
    let caps = rpc_message(&body);
    assert_eq!(
        caps["result"]["capabilities"]["resources"]["subscribe"], true,
        "{caps}"
    );
    assert!(
        caps["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("data")
    );
    let (s, _, b) = post(
        &env,
        &env.token,
        Some(&session),
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert!(s == 202 || s == 200, "{s} {b}");
    for uri in ["bitacora://page/Notes", "bitacora://page/Project%20X"] {
        let (s, _, b) = post(
            &env,
            &env.token,
            Some(&session),
            &json!({"jsonrpc":"2.0","id":2,"method":"resources/subscribe","params":{"uri":uri}}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        assert!(rpc_message(&b).get("error").is_none(), "{b}");
    }

    let listener = read_sse_until(
        &env,
        &session,
        "notifications/resources/updated",
        Duration::from_secs(15),
    );
    let touch = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let path = env.graph.join("pages/Notes.md");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("- a brand new block\n");
        std::fs::write(&path, text).unwrap();
        // The CLI drives this from a timer; the test triggers the reconcile directly.
        let started = std::time::Instant::now();
        env.indexer.reconcile().expect("reconcile");
        started
    };
    let (seen, started) = tokio::join!(listener, touch);
    let seen = seen.expect("notification arrived");
    assert!(seen.contains("bitacora://page/Notes"), "{seen}");
    assert!(started.elapsed() < Duration::from_secs(2));
    stop(env);
}

#[tokio::test(flavor = "multi_thread")]
async fn query_tool_routes_simple_and_advanced_queries() {
    let env = setup(false, false);

    // Simple DSL goes through the query compiler (`or`, `not` and `sort-by` work now).
    let r = call(
        &env,
        "query",
        json!({"dsl": "(and (or (task TODO) (task DONE)) [[Project X]])"}),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["kind"], "blocks");

    let r = call(&env, "query", json!({"dsl": "(task TODO DONE)"})).await;
    let total = r["structuredContent"]["total"].as_u64().unwrap();
    assert!(total >= 3, "{r}");

    // Pagination over the same query.
    let r = call(
        &env,
        "query",
        json!({"dsl": "(task TODO DONE)", "limit": 2}),
    )
    .await;
    assert_eq!(
        r["structuredContent"]["blocks"].as_array().unwrap().len(),
        2
    );
    assert_eq!(r["structuredContent"]["truncated"], true);
    let cursor = r["structuredContent"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let r2 = call(
        &env,
        "query",
        json!({"dsl": "(task TODO DONE)", "limit": 2, "cursor": cursor}),
    )
    .await;
    assert_ne!(
        r["structuredContent"]["blocks"][0]["uuid"],
        r2["structuredContent"]["blocks"][0]["uuid"]
    );
    let r = call(&env, "query", json!({"dsl": "(task"})).await;
    assert_eq!(r["structuredContent"]["code"], "INVALID_QUERY", "{r}");

    // Advanced: EDN map with datalog, an aggregate and `current_page`.
    let r = call(
        &env,
        "query",
        json!({"dsl": "#+BEGIN_QUERY\n{:title \"Todos\" :query [:find (pull ?b [*]) :where [?b :block/marker \"TODO\"]]}\n#+END_QUERY"}),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["kind"], "blocks");
    assert_eq!(r["structuredContent"]["title"], "Todos");
    assert!(r["structuredContent"]["total"].as_u64().unwrap() >= 2);
    let r = call(
        &env,
        "query",
        json!({"dsl": "[:find (count ?b) . :where [?b :block/marker \"TODO\"]]  "}),
    )
    .await;
    // A bare datalog vector is accepted as an advanced query.
    assert_eq!(r["structuredContent"]["kind"], "rows", "{r}");
    let r = call(
        &env,
        "query",
        json!({"dsl": "{:query [:find (count ?b) . :where [?b :block/marker \"TODO\"]]}"}),
    )
    .await;
    assert_eq!(r["structuredContent"]["kind"], "rows", "{r}");
    assert!(r["structuredContent"]["rows"][0][0].as_u64().unwrap() >= 2);
    let r = call(
        &env,
        "query",
        json!({
            "dsl": "{:query [:find (pull ?b [*]) :in $ ?p :where [?b :block/page ?pg] [?pg :block/name ?p]] :inputs [:current-page]}",
            "current_page": "Project X"
        }),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    let blocks = r["structuredContent"]["blocks"].as_array().unwrap();
    assert!(!blocks.is_empty());
    assert!(blocks.iter().all(|b| b["page"] == "Project X"), "{r}");

    // An ignored construct is a warning; the rest of the query ran.
    let r = call(
        &env,
        "query",
        json!({"dsl": "{:title \"t\" :query [:find (pull ?b [*]) :where [?b :block/marker \"TODO\"]] :result-transform (fn [r] r)}"}),
    )
    .await;
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(
        r["structuredContent"]["warnings"][0],
        "unsupported: :result-transform"
    );
    assert!(
        !r["structuredContent"]["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // A construct that cannot run at all is a structured error.
    let r = call(
        &env,
        "query",
        json!({"dsl": "{:query [:find ?b :where [?b :block/nope ?x]]}"}),
    )
    .await;
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(r["structuredContent"]["code"], "NOT_SUPPORTED");
    assert!(
        r["structuredContent"]["message"]
            .as_str()
            .unwrap()
            .starts_with("unsupported: "),
        "{r}"
    );
    stop(env);
}
