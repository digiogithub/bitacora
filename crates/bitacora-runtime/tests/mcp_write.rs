//! Agent writes end to end (BIT-US-0020..0023): a real MCP server on an ephemeral port over a temp
//! graph, wired through the runtime. A write must reach the disk atomically through the command
//! queue, show up in the index, be audited, and be undoable to the exact original bytes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::time::Duration;

use bitacora_core::queue::Source;
use bitacora_mcp::{AuditFilter, McpConfig, Scope, TokenStore};
use bitacora_runtime::{McpOptions, Session};
use common::{PAGE, config, graph_with, index_has, wait_for};
use serde_json::{Value, json};

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"writer-test","version":"1.2.3"}}}"#;

struct Env {
    s: Session,
    addr: String,
    graph: std::path::PathBuf,
    data: std::path::PathBuf,
    _tmp: tempfile::TempDir,
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

/// One raw HTTP exchange; returns `(status, body)`.
fn http(addr: &str, method: &str, path: &str, token: Option<&str>, body: &str) -> (u16, String) {
    let mut conn = std::net::TcpStream::connect(addr).unwrap();
    conn.set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n"
    );
    if let Some(t) = token {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    conn.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    conn.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body)
    } else {
        body.to_owned()
    };
    (status, body)
}

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

struct Client<'a> {
    env: &'a Env,
    token: String,
}

impl Client<'_> {
    fn rpc(&self, method: &str, params: Value) -> Value {
        let (st, body) = http(
            &self.env.addr,
            "POST",
            "/mcp",
            Some(&self.token),
            &json!({"jsonrpc":"2.0","id":7,"method":method,"params":params}).to_string(),
        );
        assert_eq!(st, 200, "{body}");
        rpc_message(&body)
    }

    /// The `result` of a tool call (`isError` results included).
    fn call(&self, tool: &str, args: Value) -> Value {
        self.rpc("tools/call", json!({"name": tool, "arguments": args}))["result"].clone()
    }

    /// A call that must succeed; returns the structured content.
    fn ok(&self, tool: &str, args: Value) -> Value {
        let r = self.call(tool, args);
        assert_ne!(r["isError"], json!(true), "{tool} failed: {r}");
        r["structuredContent"].clone()
    }

    /// A call that must fail; returns the structured error `{code, message, ...}`.
    fn err(&self, tool: &str, args: Value) -> Value {
        let r = self.call(tool, args);
        assert_eq!(r["isError"], json!(true), "{tool} should fail: {r}");
        r["structuredContent"].clone()
    }

    fn tool_names(&self) -> Vec<String> {
        self.rpc("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

struct Opts {
    mcp: McpConfig,
    files: Vec<(&'static str, &'static str)>,
    /// Put the graph under sync against a temp bare remote (fast auto-commit timings).
    sync: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            mcp: McpConfig {
                port: 0,
                stateful: false,
                allow_writes: true,
                allow_deletes: true,
                ..McpConfig::default()
            },
            files: vec![("pages/p.md", PAGE)],
            sync: false,
        }
    }
}

const ALL: &[Scope] = &[Scope::Read, Scope::Write, Scope::Delete];

fn setup(opts: Opts) -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &opts.files);
    let data = tmp.path().join("data");
    let token_path = tmp.path().join("tokens.json");
    {
        let store = TokenStore::load_or_init(&token_path).unwrap();
        store.create("agent", ALL).unwrap();
        store.create("reader", &[Scope::Read]).unwrap();
        store
            .create("writer", &[Scope::Read, Scope::Write])
            .unwrap();
    }
    let mut cfg = config(&graph, &data);
    if opts.sync {
        let remote = tmp.path().join("remote.git");
        bitacora_testkit::git::init_bare(&remote);
        let mut ob =
            bitacora_sync::onboarding::OnboardingConfig::new(bitacora_sync::detect_git(None));
        ob.identity = Some(bitacora_sync::repo_setup::Identity {
            name: "alice".into(),
            email: "alice@example.com".into(),
        });
        bitacora_sync::onboarding::enable_sync(&graph, &remote.to_string_lossy(), "main", &ob)
            .unwrap();
        let mut so = bitacora_runtime::SyncOptions::new("alice", "main");
        so.tune = Some(std::sync::Arc::new(|ec| {
            ec.commit.idle = Duration::from_millis(300);
            ec.commit.max = Duration::from_secs(2);
        }));
        cfg.sync = Some(so);
    }
    cfg.mcp = Some(McpOptions {
        config: opts.mcp,
        token_path: token_path.clone(),
        secrets: None,
    });
    let s = Session::open(cfg).unwrap();
    let addr = s
        .mcp_endpoint()
        .unwrap()
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    Env {
        s,
        addr,
        graph,
        data,
        _tmp: tmp,
    }
}

impl Env {
    fn client(&self, name: &str) -> Client<'_> {
        let store = TokenStore::load_or_init(self._tmp.path().join("tokens.json")).unwrap();
        let token = store.secret_of(name).unwrap();
        let c = Client { env: self, token };
        // Handshake (records `clientInfo`; stateless mode does not require it).
        let (st, _) = http(&self.addr, "POST", "/mcp", Some(&c.token), INIT);
        assert_eq!(st, 200);
        c
    }

    fn flush(&self) {
        self.s.queue().flush(Source::Ui).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.graph.join(rel)).unwrap()
    }

    /// Wait until the index knows `needle`.
    fn indexed(&self, needle: &str) {
        wait_for("index update", Duration::from_secs(10), || {
            index_has(&self.s, needle).then_some(())
        });
    }

    fn stop(self) {
        let r = self.s.shutdown(Duration::from_secs(10));
        assert!(r.is_clean(), "{r:?}");
    }
}

/// uuid of the block whose content is exactly `text`, from the page tree.
fn uuid_of(c: &Client<'_>, page: &str, text: &str) -> String {
    let tree = c.ok("get_page_blocks_tree", json!({"name": page}));
    fn find(blocks: &Value, text: &str) -> Option<String> {
        for b in blocks.as_array()? {
            if b["content"].as_str().is_some_and(|c| c.trim() == text) {
                return b["uuid"].as_str().map(str::to_owned);
            }
            if let Some(u) = find(&b["children"], text) {
                return Some(u);
            }
        }
        None
    }
    find(&tree["blocks"], text).unwrap_or_else(|| panic!("no block `{text}` in {tree}"))
}

fn no_tmp_files(dir: &Path) {
    let leftovers: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bitacora-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn update_block_reaches_disk_index_audit_and_undo_restores_exact_bytes() {
    let env = setup(Opts::default());
    let c = env.client("agent");
    let beta = uuid_of(&c, "p", "Beta");

    let out = c.ok(
        "update_block",
        json!({"uuid": beta, "content": "Beta edited"}),
    );
    assert_eq!(out["blocks"][0]["uuid"], json!(beta));
    let audit_id = out["audit_id"].as_str().unwrap().to_owned();

    // Written atomically through the queue; the target gets a persistent id:: lazily.
    env.flush();
    assert_eq!(
        env.read("pages/p.md"),
        format!("- Alpha\n- Beta edited\n  id:: {beta}\n- Gamma\n")
    );
    no_tmp_files(&env.graph.join("pages"));
    env.indexed("Beta edited");

    // The new version returned by the write is the one the index serves.
    let after = c.ok("get_block", json!({"uuid": beta}));
    assert_eq!(after["block"]["version"], out["blocks"][0]["version"]);

    // Audit entry: who, what, which block; never the note text.
    let entries = env.s.agent_activity(&AuditFilter::default());
    let e = entries.iter().find(|e| e.id == audit_id).unwrap();
    assert_eq!(e.tool, "update_block");
    assert_eq!(e.token.as_deref(), Some("agent"));
    // Stateless mode keeps no handshake, so clientInfo is the transport default there.
    assert!(e.client.is_some());
    assert_eq!(e.result, "ok");
    assert_eq!(e.affected, vec![beta.clone()]);
    assert!(e.undoable && !e.undone);
    assert!(!e.args.contains("Beta edited"), "{}", e.args);
    let log = std::fs::read_to_string(env.data.join("mcp-audit/audit.jsonl")).unwrap();
    assert!(log.contains(&audit_id));
    assert!(!log.contains("Beta edited"), "audit log leaks content");

    // Undo through the queue: exact original bytes, same index.
    env.s.undo_agent_entry(&audit_id).unwrap();
    env.flush();
    assert_eq!(env.read("pages/p.md"), PAGE);
    no_tmp_files(&env.graph.join("pages"));
    let e = env
        .s
        .agent_activity(&AuditFilter::default())
        .into_iter()
        .find(|e| e.id == audit_id)
        .unwrap();
    assert!(e.undone && !e.undoable);
    assert!(env.s.undo_agent_entry(&audit_id).is_err(), "no double undo");
    env.stop();
}

#[test]
fn optimistic_concurrency_and_validation_errors() {
    let env = setup(Opts::default());
    let c = env.client("agent");
    let beta = uuid_of(&c, "p", "Beta");
    let block = c.ok("get_block", json!({"uuid": beta}))["block"].clone();
    let version = block["version"].as_str().unwrap().to_owned();

    // Stale version -> CONFLICT carrying the current block.
    let e = c.err(
        "update_block",
        json!({"uuid": beta, "content": "x", "expected_version": "0000000000000000"}),
    );
    assert_eq!(e["code"], "CONFLICT");
    assert_eq!(e["current"]["uuid"], json!(beta));
    assert!(e["current"]["content"].as_str().unwrap().contains("Beta"));

    // Content rules.
    for bad in [
        "- one\n- two",
        "a\nid:: 11111111-1111-4111-8111-111111111111",
        "  ",
        "a\n- b",
    ] {
        let e = c.err("update_block", json!({"uuid": beta, "content": bad}));
        assert_eq!(e["code"], "INVALID_CONTENT", "{bad}");
    }
    let e = c.err(
        "set_block_property",
        json!({"uuid": beta, "key": "id", "value": "x"}),
    );
    assert_eq!(e["code"], "INVALID_CONTENT");
    let e = c.err(
        "update_block",
        json!({"uuid": "00000000-0000-4000-8000-000000000000", "content": "x"}),
    );
    assert_eq!(e["code"], "NOT_FOUND");

    // Matching version succeeds; reusing the old one afterwards conflicts.
    c.ok(
        "update_block",
        json!({"uuid": beta, "content": "Beta v2", "expected_version": version}),
    );
    let e = c.err(
        "update_block",
        json!({"uuid": beta, "content": "Beta v3", "expected_version": version}),
    );
    assert_eq!(e["code"], "CONFLICT");
    assert!(
        e["current"]["content"]
            .as_str()
            .unwrap()
            .contains("Beta v2")
    );

    // Nothing leaked into the file by the failed calls.
    env.flush();
    assert!(!env.read("pages/p.md").contains("v3"));
    env.stop();
}

#[test]
fn toggles_scopes_catalogue_rate_limit_and_protected_pages() {
    // Writes off by default: READ_ONLY, nothing written, write tools hidden.
    let off = setup(Opts {
        mcp: McpConfig {
            port: 0,
            stateful: false,
            ..McpConfig::default()
        },
        ..Opts::default()
    });
    let c = off.client("agent");
    let beta = uuid_of(&c, "p", "Beta");
    assert_eq!(
        c.err("update_block", json!({"uuid": beta, "content": "x"}))["code"],
        "READ_ONLY"
    );
    assert_eq!(
        c.err("remove_block", json!({"uuid": beta}))["code"],
        "READ_ONLY"
    );
    assert!(
        !c.tool_names()
            .iter()
            .any(|n| n == "update_block" || n == "remove_block")
    );
    assert!(c.tool_names().iter().any(|n| n == "search"));
    off.flush();
    assert_eq!(off.read("pages/p.md"), PAGE);
    // Flip the toggles at runtime.
    let policy = off.s.mcp_policy().unwrap();
    policy.set_allow_writes(true);
    assert!(c.tool_names().iter().any(|n| n == "update_block"));
    assert!(
        !c.tool_names().iter().any(|n| n == "remove_block"),
        "deletes still off"
    );
    assert_eq!(
        c.err("remove_block", json!({"uuid": beta}))["code"],
        "READ_ONLY"
    );
    policy.set_allow_deletes(true);
    assert!(c.tool_names().iter().any(|n| n == "remove_block"));
    off.stop();

    // Scopes: a write-only token cannot delete, a reader cannot write.
    let env = setup(Opts {
        mcp: McpConfig {
            port: 0,
            stateful: false,
            allow_writes: true,
            allow_deletes: true,
            protected_namespaces: vec!["Private".into()],
            ..McpConfig::default()
        },
        files: vec![
            ("logseq/config.edn", "{:file/name-format :triple-lowbar}\n"),
            ("pages/p.md", PAGE),
            ("pages/Private___Diary.md", "- secret\n"),
            (
                "pages/Locked.md",
                "bitacora-agent-readonly:: true\n\n- keep me\n",
            ),
        ],
        sync: false,
    });
    let w = env.client("writer");
    let r = env.client("reader");
    let beta = uuid_of(&w, "p", "Beta");
    assert_eq!(
        r.err("update_block", json!({"uuid": beta, "content": "x"}))["code"],
        "FORBIDDEN_SCOPE"
    );
    assert_eq!(
        w.err("remove_block", json!({"uuid": beta}))["code"],
        "FORBIDDEN_SCOPE"
    );
    assert!(!w.tool_names().iter().any(|n| n == "remove_block"));
    assert!(!r.tool_names().iter().any(|n| n == "append_block"));

    // Protected namespace and read-only marker.
    let secret = uuid_of(&w, "Private/Diary", "secret");
    assert_eq!(
        w.err("update_block", json!({"uuid": secret, "content": "x"}))["code"],
        "PROTECTED_PAGE"
    );
    assert_eq!(
        w.err(
            "append_block",
            json!({"page": "Private/Diary", "content": "x"})
        )["code"],
        "PROTECTED_PAGE"
    );
    let keep = uuid_of(&w, "Locked", "keep me");
    assert_eq!(
        w.err("update_block", json!({"uuid": keep, "content": "x"}))["code"],
        "PROTECTED_PAGE"
    );
    // Content limit.
    let many: Vec<Value> = (0..201)
        .map(|i| json!({"content": format!("b{i}")}))
        .collect();
    assert_eq!(
        w.err("append_block", json!({"page": "p", "blocks": many}))["code"],
        "INVALID_CONTENT"
    );

    env.flush();
    assert_eq!(env.read("pages/Private___Diary.md"), "- secret\n");
    assert_eq!(
        env.read("pages/Locked.md"),
        "bitacora-agent-readonly:: true\n\n- keep me\n"
    );
    env.stop();

    // Rate limit: 3 write operations per minute per token.
    let env = setup(Opts {
        mcp: McpConfig {
            port: 0,
            stateful: false,
            allow_writes: true,
            writes_per_minute: 3,
            ..McpConfig::default()
        },
        ..Opts::default()
    });
    let w = env.client("writer");
    for i in 0..3 {
        w.ok(
            "append_block",
            json!({"page": "p", "content": format!("n{i}")}),
        );
    }
    let e = w.err("append_block", json!({"page": "p", "content": "n3"}));
    assert_eq!(e["code"], "RATE_LIMITED");
    assert!(e["retry_after_ms"].as_u64().unwrap() > 0);
    // Another token has its own budget; reads are not limited.
    env.client("agent")
        .ok("append_block", json!({"page": "p", "content": "other"}));
    w.ok("search", json!({"query": "Alpha"}));
    env.flush();
    assert!(!env.read("pages/p.md").contains("n3"));
    env.stop();
}

/// Flush and wait until the index serves `needle`, so freshly read uuids are current.
fn settle(env: &Env, needle: &str) {
    env.flush();
    env.indexed(needle);
}

#[test]
fn insert_move_properties_status_remove_and_group_undo() {
    let env = setup(Opts::default());
    let c = env.client("agent");
    let alpha = uuid_of(&c, "p", "Alpha");

    // A tree after Alpha: one call, one audit entry, one undo step.
    let out = c.ok(
        "insert_block",
        json!({"target_uuid": alpha, "position": "after",
               "content": "Root\nsecond line", "properties": {"kind": "demo"},
               "children": [{"content": "Child A"}, {"content": "Child B", "children": [{"content": "Deep"}]}]}),
    );
    assert_eq!(out["blocks"].as_array().unwrap().len(), 4);
    let insert_audit = out["audit_id"].as_str().unwrap().to_owned();
    let root = out["blocks"][0]["uuid"].as_str().unwrap().to_owned();

    // Blocks created by the agent are addressable at once, before the index caught up.
    c.ok("set_task_status", json!({"uuid": root, "status": "TODO"}));
    c.ok(
        "set_block_property",
        json!({"uuid": root, "key": "priority-note", "value": "high"}),
    );
    c.ok(
        "remove_block_property",
        json!({"uuid": root, "key": "kind"}),
    );
    settle(&env, "priority-note");

    // Fresh uuids from the index for the blocks that were not touched by an agent.
    let (alpha, gamma) = (uuid_of(&c, "p", "Alpha"), uuid_of(&c, "p", "Gamma"));
    c.ok(
        "move_block",
        json!({"uuid": gamma, "target_uuid": alpha, "position": "before"}),
    );
    c.ok("prepend_block", json!({"page": "p", "content": "First"}));
    c.ok(
        "insert_block",
        json!({"target_uuid": alpha, "position": "first_child", "content": "Kid"}),
    );
    env.flush();
    let text = env.read("pages/p.md");
    assert!(text.starts_with("- First\n"), "{text}");
    let pos = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle} in {text}"))
    };
    assert!(pos("- Gamma") < pos("- Alpha"), "{text}");
    assert!(pos("- Alpha") < pos("Kid"), "{text}");
    // Properties follow the title line (Logseq layout), before the continuation lines.
    assert!(text.contains("TODO Root\n  id:: "), "{text}");
    assert!(pos("second line") > pos("priority-note:: high"), "{text}");
    assert!(text.contains("priority-note:: high"), "{text}");
    assert!(!text.contains("kind:: demo"), "{text}");
    assert!(
        pos("Child A") < pos("Child B") && pos("Child B") < pos("Deep"),
        "{text}"
    );
    settle(&env, "Kid");

    // Remove the tree (children go too) and undo exactly that call.
    let before_remove = env.read("pages/p.md");
    let rm = c.ok("remove_block", json!({"uuid": root}));
    env.flush();
    assert!(!env.read("pages/p.md").contains("Child B"));
    env.s
        .undo_agent_entry(rm["audit_id"].as_str().unwrap())
        .unwrap();
    env.flush();
    assert_eq!(env.read("pages/p.md"), before_remove);

    // The earlier insert cannot be undone any more: its blocks were edited since. Nothing changes.
    assert!(env.s.undo_agent_entry(&insert_audit).is_err());
    env.flush();
    assert_eq!(env.read("pages/p.md"), before_remove);
    env.stop();
}

#[test]
fn a_multi_block_insert_undoes_as_one_step() {
    let env = setup(Opts::default());
    let c = env.client("agent");
    let alpha = uuid_of(&c, "p", "Alpha");
    let out = c.ok(
        "insert_block",
        json!({"target_uuid": alpha, "position": "last_child",
               "blocks": [{"content": "one", "children": [{"content": "one-a"}]}, {"content": "two"}]}),
    );
    env.flush();
    assert!(env.read("pages/p.md").contains("\t- one-a"));
    env.s
        .undo_agent_entry(out["audit_id"].as_str().unwrap())
        .unwrap();
    env.flush();
    assert_eq!(env.read("pages/p.md"), PAGE);
    env.stop();
}

#[test]
fn create_page_append_today_rename_with_links_delete_and_undo() {
    let env = setup(Opts {
        files: vec![
            ("pages/p.md", PAGE),
            ("pages/Old Name.md", "- about Old Name\n  alias-note:: x\n"),
            (
                "pages/Linker.md",
                "- see [[Old Name]] and #[[Old Name]] and [[other]]\n",
            ),
        ],
        ..Opts::default()
    });
    let c = env.client("agent");
    env.indexed("and [[other]]");

    // create_page: properties + blocks; a second create errors, `return` is idempotent.
    let out = c.ok(
        "create_page",
        json!({"name": "Fresh Page", "properties": {"type": "note"}, "blocks": [{"content": "hello", "children": [{"content": "world"}]}]}),
    );
    assert_eq!(out["created"], json!(true));
    assert_eq!(
        c.err("create_page", json!({"name": "Fresh Page"}))["code"],
        "CONFLICT"
    );
    env.flush();
    let text = env.read("pages/Fresh Page.md");
    assert!(text.starts_with("type:: note\n\n- hello"), "{text}");
    assert!(text.contains("\t- world"), "{text}");
    env.indexed("world");
    let again = c.ok(
        "create_page",
        json!({"name": "fresh page", "if_exists": "return"}),
    );
    assert_eq!(again["created"], json!(false));

    // append to today's journal creates the journal file under the configured name.
    let out = c.ok(
        "append_block",
        json!({"page": "today", "content": "journal note"}),
    );
    env.flush();
    let day = jiff::Zoned::now().date();
    let rel = format!(
        "journals/{:04}_{:02}_{:02}.md",
        day.year(),
        day.month(),
        day.day()
    );
    assert!(env.read(&rel).contains("journal note"), "{}", out);

    // rename: new file, old file recycled, links rewritten, one undo step.
    let before_link = env.read("pages/Linker.md");
    let rn = c.ok(
        "rename_page",
        json!({"name": "Old Name", "new_name": "New Name"}),
    );
    assert_eq!(rn["details"]["rewritten_pages"], json!(["Linker"]));
    env.flush();
    assert!(env.read("pages/New Name.md").contains("alias-note:: x"));
    assert!(!env.graph.join("pages/Old Name.md").exists());
    assert_eq!(
        env.read("pages/Linker.md"),
        "- see [[New Name]] and #[[New Name]] and [[other]]\n"
    );
    env.s
        .undo_agent_entry(rn["audit_id"].as_str().unwrap())
        .unwrap();
    env.flush();
    assert_eq!(
        env.read("pages/Old Name.md"),
        "- about Old Name\n  alias-note:: x\n"
    );
    assert_eq!(env.read("pages/Linker.md"), before_link);
    assert!(!env.graph.join("pages/New Name.md").exists());

    // rename onto an existing page: structured CONFLICT, nothing changes, unless `merge`.
    c.ok(
        "create_page",
        json!({"name": "Taken", "blocks": [{"content": "taken block"}]}),
    );
    env.flush();
    let refused = c.err(
        "rename_page",
        json!({"name": "Old Name", "new_name": "Taken"}),
    );
    assert_eq!(refused["code"], "CONFLICT");
    assert_eq!(refused["target_exists"], "Taken", "{refused}");
    assert!(env.graph.join("pages/Old Name.md").exists());
    // Journals are refused by core.
    assert_eq!(
        c.err(
            "rename_page",
            json!({"name": "Old Name", "new_name": "2024-01-02"})
        )["code"],
        "INVALID_ARGUMENT"
    );
    let merged = c.ok(
        "rename_page",
        json!({"name": "Old Name", "new_name": "Taken", "merge": true}),
    );
    assert_eq!(merged["details"]["merged"], json!(true), "{merged}");
    env.flush();
    let taken = env.read("pages/Taken.md");
    assert!(
        taken.contains("taken block") && taken.contains("Old Name"),
        "{taken}"
    );
    assert!(!env.graph.join("pages/Old Name.md").exists());
    env.s
        .undo_agent_entry(merged["audit_id"].as_str().unwrap())
        .unwrap();
    env.flush();
    assert_eq!(
        env.read("pages/Old Name.md"),
        "- about Old Name\n  alias-note:: x\n"
    );
    assert!(!env.read("pages/Taken.md").contains("about Old Name"));

    // delete_page recycles the file; undo brings it back byte for byte.
    let del = c.ok("delete_page", json!({"name": "Linker"}));
    env.flush();
    assert!(!env.graph.join("pages/Linker.md").exists());
    env.s
        .undo_agent_entry(del["audit_id"].as_str().unwrap())
        .unwrap();
    env.flush();
    assert_eq!(env.read("pages/Linker.md"), before_link);
    env.stop();
}

#[test]
fn audit_covers_reads_and_auth_failures_and_survives_restart() {
    let env = setup(Opts::default());
    let c = env.client("reader");
    c.ok("search", json!({"query": "Alpha"}));
    let (st, _) = http(&env.addr, "POST", "/mcp", Some("bit_wrong"), "{}");
    assert_eq!(st, 401);
    let (st, _) = http(&env.addr, "POST", "/mcp", None, "{}");
    assert_eq!(st, 401);
    let all = env.s.agent_activity(&AuditFilter {
        limit: Some(1000),
        ..AuditFilter::default()
    });
    assert!(
        all.iter().any(|e| e.tool == "search"
            && e.token.as_deref() == Some("reader")
            && e.result == "ok")
    );
    assert_eq!(all.iter().filter(|e| e.tool == "auth").count(), 2);
    let log = std::fs::read_to_string(env.data.join("mcp-audit/audit.jsonl")).unwrap();
    assert!(
        !log.contains("bit_wrong") && !log.contains("Bearer"),
        "token values must not be logged"
    );

    // Filters.
    let only_search = env.s.agent_activity(&AuditFilter {
        tool: Some("search".into()),
        ..AuditFilter::default()
    });
    assert!(only_search.iter().all(|e| e.tool == "search") && !only_search.is_empty());
    env.stop();
}

#[test]
fn compat_api_is_off_by_default_and_audited_when_on() {
    // Off: 404 even with a valid token.
    let off = setup(Opts::default());
    let token = off.client("agent").token.clone();
    let (st, _) = http(
        &off.addr,
        "POST",
        "/api",
        Some(&token),
        r#"{"method":"logseq.Editor.getPage","args":["p"]}"#,
    );
    assert_eq!(st, 404);
    off.stop();

    let env = setup(Opts {
        mcp: McpConfig {
            port: 0,
            stateful: false,
            allow_writes: true,
            allow_deletes: true,
            api_enabled: true,
            ..McpConfig::default()
        },
        ..Opts::default()
    });
    let token = env.client("agent").token.clone();
    let api = |method: &str, args: Value| -> (u16, Value) {
        let (st, body) = http(
            &env.addr,
            "POST",
            "/api",
            Some(&token),
            &json!({"method": method, "args": args}).to_string(),
        );
        (st, serde_json::from_str(&body).unwrap_or(Value::Null))
    };
    // Same guard: no token, wrong token.
    let (st, _) = http(&env.addr, "POST", "/api", None, "{}");
    assert_eq!(st, 401);

    let (st, page) = api("logseq.Editor.getPage", json!(["p"]));
    assert_eq!(st, 200);
    assert_eq!(page["originalName"], "p");
    let (_, tree) = api("logseq.Editor.getPageBlocksTree", json!(["p"]));
    let beta = tree[1]["uuid"].as_str().unwrap().to_owned();
    assert_eq!(tree[1]["page"]["name"], "p");
    let (_, block) = api("logseq.Editor.getBlock", json!([beta]));
    assert_eq!(block["uuid"], json!(beta));
    let (_, missing) = api(
        "logseq.Editor.getBlock",
        json!(["00000000-0000-4000-8000-000000000000"]),
    );
    assert_eq!(missing, Value::Null);

    // Writes map onto the same pipeline.
    let (_, ins) = api(
        "logseq.Editor.insertBlock",
        json!([beta, "via api", {"sibling": true}]),
    );
    assert!(ins["uuid"].is_string(), "{ins}");
    api("logseq.Editor.appendBlockInPage", json!(["p", "tail"]));
    api(
        "logseq.Editor.upsertBlockProperty",
        json!([beta, "status", "open"]),
    );
    env.flush();
    let text = env.read("pages/p.md");
    assert!(
        text.contains("- via api") && text.contains("- tail") && text.contains("status:: open"),
        "{text}"
    );
    env.indexed("via api");
    let (_, q) = api("logseq.DB.q", json!(["\"via api\""]));
    assert!(q.as_array().is_some_and(|a| !a.is_empty()), "{q}");
    let (_, hits) = api("logseq.search", json!(["tail"]));
    assert!(hits.as_array().is_some_and(|a| !a.is_empty()), "{hits}");

    // Unsupported surface.
    for m in [
        "logseq.UI.showMsg",
        "logseq.Git.execCommand",
        "logseq.App.relaunch",
        "logseq.DB.datascriptQuery",
        "logseq.Editor.nope",
        "plugin.thing",
    ] {
        let (st, v) = api(m, json!([]));
        assert_eq!(
            (st, v["error"].as_str()),
            (200, Some("method not supported")),
            "{m}"
        );
    }

    // Audited like MCP calls, including the undo data of writes.
    let entries = env.s.agent_activity(&AuditFilter {
        limit: Some(1000),
        ..AuditFilter::default()
    });
    let ins_entry = entries
        .iter()
        .find(|e| e.tool == "api:logseq.Editor.insertBlock")
        .unwrap();
    assert!(ins_entry.write && ins_entry.undoable);
    // Earlier entries are refused once the page moved on; the newest one undoes.
    assert!(env.s.undo_agent_entry(&ins_entry.id).is_err());
    let last = entries
        .iter()
        .find(|e| e.tool == "api:logseq.Editor.upsertBlockProperty")
        .unwrap();
    env.s.undo_agent_entry(&last.id).unwrap();
    env.flush();
    let text = env.read("pages/p.md");
    assert!(
        !text.contains("status:: open") && text.contains("via api"),
        "{text}"
    );
    env.stop();
}

#[test]
fn catalogue_has_no_shell_or_raw_path_tools() {
    let env = setup(Opts::default());
    let c = env.client("agent");
    let tools = c.rpc("tools/list", json!({}))["result"]["tools"].clone();
    let tools = tools.as_array().unwrap();
    assert!(tools.len() >= 25, "{} tools", tools.len());
    // The only sync tools are the status read and the "sync now" trigger.
    let allowed = ["git_sync_status", "git_sync_now"];
    for t in tools {
        let name = t["name"].as_str().unwrap();
        for bad in [
            "exec",
            "shell",
            "command",
            "run_",
            "bash",
            "eval",
            "raw",
            "write_file",
            "read_file",
        ] {
            assert!(!name.contains(bad), "tool `{name}` looks like a shell tool");
        }
        if name.contains("git") {
            assert!(allowed.contains(&name), "unexpected sync tool `{name}`");
        }
        // No argument takes a writable file path or a command line.
        let props = t["inputSchema"]["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for key in props.keys() {
            for bad in ["path", "file", "command", "cmd", "shell", "argv", "script"] {
                assert!(
                    !key.contains(bad),
                    "tool `{name}` takes a `{key}` argument that could carry a path or command"
                );
            }
        }
        if allowed.contains(&name) {
            assert!(props.keys().all(|k| k == "graph"), "{name}: {props:?}");
        }
    }
    env.stop();
}

#[test]
fn agent_write_is_committed_by_sync_as_kind_agent_with_the_token_name() {
    assert!(bitacora_testkit::git::git_available());
    let env = setup(Opts {
        sync: true,
        ..Opts::default()
    });
    let c = env.client("agent");
    let beta = uuid_of(&c, "p", "Beta");
    c.ok(
        "update_block",
        json!({"uuid": beta, "content": "Beta by agent"}),
    );
    // The idle debounce flushes and commits on its own; no manual sync call.
    let msg = wait_for("agent commit", Duration::from_secs(20), || {
        let m = bitacora_testkit::git::git(&env.graph, &["log", "-1", "--format=%B"]);
        m.contains("Bitacora-Kind: agent").then_some(m)
    });
    assert!(msg.contains("Bitacora-Agent: agent"), "{msg}");
    assert!(env.read("pages/p.md").contains("Beta by agent"));
    // A sync-now request is accepted while the engine runs.
    let r = c.ok("git_sync_now", json!({}));
    assert_eq!(r["requested"], json!(true));
    env.stop();
}
