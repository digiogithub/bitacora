#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Journal review and recommender runs against an axum SSE mock: recorded-style answers are
//! parsed, invalid output surfaces as an error, unknown tasks and spans are dropped, the review
//! cache avoids new runs and nothing is sent without consent (BIT-T-0461/0463/0464).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use bitacora_config::pando::GraphConsent;
use bitacora_pando::agents::AgentError;
use bitacora_pando::agents::lookup::StaticLookup;
use bitacora_pando::agents::recommend::run_recommend;
use bitacora_pando::agents::{
    AttachedBlock, BlockInfo, ContentGuard, RecommendDeps, RecommendRequest, ReviewCache,
    ReviewDeps, ReviewRange, run_review,
};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

type Script = Arc<dyn Fn(usize, &Value) -> Vec<Value> + Send + Sync>;

#[derive(Clone)]
struct Mock {
    runs: Arc<Mutex<Vec<Value>>>,
    script: Script,
}

fn sse(frames: &[Value]) -> Response {
    let mut body = String::new();
    for f in frames {
        body.push_str(&format!("data: {f}\n\n"));
    }
    ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response()
}

async fn serve(script: Script) -> (pando::agui::AguiClient, Arc<Mutex<Vec<Value>>>) {
    let mock = Mock {
        runs: Arc::default(),
        script,
    };
    let runs = mock.runs.clone();
    let app = Router::new()
        .route(
            "/api/v1/agui/{agent}",
            post(
                |State(m): State<Mock>,
                 Path(agent): Path<String>,
                 axum::Json(b): axum::Json<Value>| async move {
                    let n = {
                        let mut r = m.runs.lock().unwrap();
                        let mut b = b.clone();
                        b["_agent"] = json!(agent);
                        r.push(b);
                        r.len()
                    };
                    sse(&(m.script)(n, &b))
                },
            ),
        )
        .route(
            "/api/v1/agui/runs/{id}/cancel",
            post(|| async { axum::http::StatusCode::NO_CONTENT }),
        )
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let client = PandoClient::new(PandoConfig::new(format!("http://{addr}")).with_token("t"))
        .unwrap()
        .agui();
    (client, runs)
}

/// An answering script: the assistant says `answer` (split in two deltas).
fn answer(answer: &str) -> Script {
    let answer = answer.to_owned();
    Arc::new(move |n, b| {
        let t = &b["threadId"];
        let (head, tail) = answer.split_at(answer.len() / 2);
        vec![
            json!({"type": "RUN_STARTED", "threadId": t, "runId": format!("r{n}")}),
            json!({"type": "TEXT_MESSAGE_START", "messageId": "m", "role": "assistant"}),
            json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m", "delta": head}),
            json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m", "delta": tail}),
            json!({"type": "TEXT_MESSAGE_END", "messageId": "m"}),
            json!({"type": "RUN_FINISHED", "threadId": t, "runId": format!("r{n}"), "outcome": "success"}),
        ]
    })
}

fn guard(granted: bool) -> ContentGuard {
    ContentGuard::from_consent(&GraphConsent {
        granted,
        exclusions: vec!["Secrets".into()],
        ..GraphConsent::default()
    })
}

fn journal(uuid: &str, marker: Option<&str>, text: &str, day: i64) -> BlockInfo {
    BlockInfo {
        block: AttachedBlock {
            page: format!("Journal {day}"),
            file_path: format!("journals/{day}.md"),
            tags: Vec::new(),
            uuid: Some(uuid.into()),
            text: text.into(),
            page_private: false,
        },
        marker: marker.map(str::to_owned),
        journal_day: Some(day),
    }
}

fn page_block(uuid: &str, page: &str, text: &str) -> BlockInfo {
    BlockInfo {
        block: AttachedBlock {
            page: page.into(),
            file_path: format!("pages/{page}.md"),
            tags: Vec::new(),
            uuid: Some(uuid.into()),
            text: text.into(),
            page_private: false,
        },
        marker: None,
        journal_day: None,
    }
}

const DAY: i64 = 20_261_007;

const REVIEW_ANSWER: &str = "Here is the review:\n```json\n{\"summary\": \"A calm day\", \
    \"themes\": [\"rust\", \"planning\"], \"mood\": \"focused\", \
    \"pending_tasks\": [{\"block_uuid\": \"t1\"}, {\"block_uuid\": \"t2\"}, {\"block_uuid\": \"ghost\"}, {\"block_uuid\": \"hid\"}], \
    \"next_actions\": [\"write the report\"]}\n```";

fn lookup() -> Arc<StaticLookup> {
    Arc::new(
        StaticLookup::new()
            .with(journal("t1", Some("TODO"), "TODO write the report", DAY))
            .with(journal("t2", Some("DONE"), "DONE ship", DAY))
            .with(journal("n1", None, "walked the dog", DAY))
            .with(BlockInfo {
                block: AttachedBlock {
                    page: "Secrets".into(),
                    file_path: "pages/Secrets.md".into(),
                    tags: Vec::new(),
                    uuid: Some("hid".into()),
                    text: "TODO hidden".into(),
                    page_private: false,
                },
                marker: Some("TODO".into()),
                journal_day: Some(DAY),
            }),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn review_parses_a_valid_run_and_drops_unknown_tasks() {
    let (agui, runs) = serve(answer(REVIEW_ANSWER)).await;
    let deps = ReviewDeps::new(agui, lookup(), guard(true), "graph-1");
    let report = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap();
    assert!(!report.from_cache);
    assert_eq!(report.review.summary, "A calm day");
    assert_eq!(report.review.themes, ["rust", "planning"]);
    assert_eq!(report.review.mood.as_deref(), Some("focused"));
    assert_eq!(report.review.next_actions, ["write the report"]);
    // Only the open task of the index survives; its text comes from the index.
    assert_eq!(report.review.pending_tasks.len(), 1);
    assert_eq!(report.review.pending_tasks[0].block_uuid, "t1");
    assert_eq!(report.review.pending_tasks[0].text, "TODO write the report");
    assert_eq!(report.dropped_tasks, ["t2", "ghost", "hid"]);

    let r = runs.lock().unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0]["_agent"], "bitacora-journal-reviewer");
    let prompt = r[0]["messages"][0]["content"].as_str().unwrap();
    assert!(prompt.contains("2026-10-07"));
    // No graph content is sent as context, and no frontend tools are declared.
    assert!(r[0]["context"].as_array().is_none_or(Vec::is_empty));
    assert!(r[0]["tools"].as_array().is_none_or(Vec::is_empty));
}

#[tokio::test(flavor = "multi_thread")]
async fn review_surfaces_invalid_output_as_an_error() {
    for bad in ["I could not read your journal.", "{\"themes\": [\"x\"]}"] {
        let (agui, _) = serve(answer(bad)).await;
        let deps = ReviewDeps::new(agui, lookup(), guard(true), "g");
        let err = run_review(&deps, ReviewRange::day(DAY), false)
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::InvalidOutput(_)), "{bad}: {err}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn review_without_consent_sends_nothing() {
    let (agui, runs) = serve(answer(REVIEW_ANSWER)).await;
    let deps = ReviewDeps::new(agui, lookup(), guard(false), "g");
    let err = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap_err();
    assert!(matches!(err, AgentError::Unavailable(_)));
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn cached_review_is_served_without_a_new_run_until_the_journal_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (agui, runs) = serve(answer(REVIEW_ANSWER)).await;
    let mut deps = ReviewDeps::new(agui, lookup(), guard(true), "g");
    deps.cache = Some(ReviewCache::new(dir.path()));

    let first = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap();
    assert!(!first.from_cache);
    let second = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap();
    assert!(second.from_cache);
    assert_eq!(second.review, first.review);
    assert_eq!(runs.lock().unwrap().len(), 1, "the second call made no run");
    // The cache is a machine-local file, never inside the graph.
    assert!(dir.path().join("agent-reviews.json").is_file());

    // Forcing re-runs; a new instance over the same dir still hits.
    let forced = run_review(&deps, ReviewRange::day(DAY), true)
        .await
        .unwrap();
    assert!(!forced.from_cache);
    assert_eq!(runs.lock().unwrap().len(), 2);
    let mut again = deps.clone();
    again.cache = Some(ReviewCache::new(dir.path()));
    assert!(
        run_review(&again, ReviewRange::day(DAY), false)
            .await
            .unwrap()
            .from_cache
    );

    // Editing the journal changes the content hash: the cached review is stale.
    let mut edited = deps.clone();
    edited.lookup = Arc::new(StaticLookup::new().with(journal(
        "t1",
        Some("TODO"),
        "TODO write the report v2",
        DAY,
    )));
    let fresh = run_review(&edited, ReviewRange::day(DAY), false)
        .await
        .unwrap();
    assert!(!fresh.from_cache);
    assert_eq!(runs.lock().unwrap().len(), 3);
    // Another range is another key.
    let other = run_review(
        &deps,
        ReviewRange::parse("2026-10-01", "2026-10-07").unwrap(),
        false,
    )
    .await
    .unwrap();
    assert!(!other.from_cache);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_shot_runs_deny_permission_prompts_instead_of_blocking() {
    let script: Script = Arc::new(|n, b| {
        let t = &b["threadId"];
        let mut out = vec![json!({"type": "RUN_STARTED", "threadId": t, "runId": format!("r{n}")})];
        if n == 1 {
            out.extend([
                json!({"type": "TOOL_CALL_START", "toolCallId": "p1", "toolCallName": "pando_permission_request"}),
                json!({"type": "TOOL_CALL_ARGS", "toolCallId": "p1", "delta": "{\"toolName\":\"bash\"}"}),
                json!({"type": "TOOL_CALL_END", "toolCallId": "p1"}),
                json!({"type": "RUN_FINISHED", "threadId": t, "runId": "r1", "outcome": "interrupt"}),
            ]);
        } else {
            out.extend([
                json!({"type": "TEXT_MESSAGE_START", "messageId": "m", "role": "assistant"}),
                json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m", "delta": "{\"summary\": \"ok\"}"}),
                json!({"type": "RUN_FINISHED", "threadId": t, "runId": "r2", "outcome": "success"}),
            ]);
        }
        out
    });
    let (agui, runs) = serve(script).await;
    let deps = ReviewDeps::new(agui, lookup(), guard(true), "g");
    let report = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap();
    assert_eq!(report.review.summary, "ok");
    let r = runs.lock().unwrap();
    let last = r[1]["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["role"], "tool");
    assert_eq!(last["content"], "{\"approved\":false}");
}

/// Prompts of the allow-listed read tools are approved (a real Pando asks before every MCP call),
/// every other prompt is denied, and the server's own parked tool calls are never answered.
#[tokio::test(flavor = "multi_thread")]
async fn one_shot_runs_approve_only_allow_listed_read_tools() {
    let script: Script = Arc::new(|n, b| {
        let t = &b["threadId"];
        let call = |id: &str, name: &str, args: &str| {
            vec![
                json!({"type": "TOOL_CALL_START", "toolCallId": id, "toolCallName": name}),
                json!({"type": "TOOL_CALL_ARGS", "toolCallId": id, "delta": args}),
                json!({"type": "TOOL_CALL_END", "toolCallId": id}),
            ]
        };
        let mut out = vec![json!({"type": "RUN_STARTED", "threadId": t, "runId": format!("r{n}")})];
        match n {
            1 => {
                out.extend(call("srv", "bitacora_search", "{}"));
                out.extend(call(
                    "p1",
                    "pando_permission_request",
                    "{\"toolName\":\"bitacora_search\"}",
                ));
                out.push(json!({"type": "RUN_FINISHED", "threadId": t, "runId": "r1", "outcome": "interrupt"}));
            }
            2 => {
                out.extend(call(
                    "p2",
                    "pando_permission_request",
                    "{\"toolName\":\"bitacora_append_block\"}",
                ));
                out.push(json!({"type": "RUN_FINISHED", "threadId": t, "runId": "r2", "outcome": "interrupt"}));
            }
            _ => out.extend([
                json!({"type": "TEXT_MESSAGE_START", "messageId": "m", "role": "assistant"}),
                json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m", "delta": "{\"summary\": \"ok\"}"}),
                json!({"type": "RUN_FINISHED", "threadId": t, "runId": "r3", "outcome": "success"}),
            ]),
        }
        out
    });
    let (agui, runs) = serve(script).await;
    let deps = ReviewDeps::new(agui, lookup(), guard(true), "g");
    run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap();
    let r = runs.lock().unwrap();
    assert_eq!(r.len(), 3);
    let tool_msgs = |run: usize| -> Vec<Value> {
        r[run]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "tool")
            .cloned()
            .collect()
    };
    let first = tool_msgs(1);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0]["toolCallId"], "p1");
    assert_eq!(first[0]["content"], "{\"approved\":true}");
    let second = tool_msgs(2);
    assert_eq!(second.len(), 2, "{second:?}");
    assert_eq!(second[1]["toolCallId"], "p2");
    assert_eq!(second[1]["content"], "{\"approved\":false}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_without_any_text_is_invalid_output() {
    let script: Script = Arc::new(|n, b| {
        vec![
            json!({"type": "RUN_STARTED", "threadId": b["threadId"], "runId": format!("r{n}")}),
            json!({"type": "RUN_FINISHED", "threadId": b["threadId"], "runId": format!("r{n}")}),
        ]
    });
    let (agui, _) = serve(script).await;
    let mut deps = ReviewDeps::new(agui, lookup(), guard(true), "g");
    deps.timeout = Duration::from_secs(5);
    let err = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap_err();
    assert!(matches!(err, AgentError::InvalidOutput(_)), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn recommender_drops_invalid_spans_and_hidden_targets() {
    let answer_json = json!({
        "related_pages": [{"page": "Helm", "reason": "charts"}, {"page": "Secrets", "reason": "x"}],
        "link_suggestions": [
            {"block_uuid": "b1", "text": "kubernetes", "target": "Kubernetes"},
            {"block_uuid": "b1", "text": "terraform", "target": "Terraform"},
            {"block_uuid": "b2", "text": "kubernetes", "target": "Kubernetes"},
            {"block_uuid": "b1", "text": "helm", "target": "Secrets"}
        ],
        "tag_suggestions": [{"tag": "#devops", "reason": "infra"}],
        "next_actions": ["write the runbook"]
    })
    .to_string();
    let (agui, runs) = serve(answer(&format!("Sure.\n{answer_json}"))).await;
    let lookup = Arc::new(
        StaticLookup::new()
            .with(page_block("b1", "Cluster", "Deploy kubernetes with helm"))
            .with(page_block("b2", "Other", "kubernetes elsewhere")),
    );
    let deps = RecommendDeps::new(agui, lookup, guard(true));
    let req = RecommendRequest {
        page: "Cluster".into(),
    };
    let out = run_recommend(&deps, &req).await.unwrap();
    assert_eq!(out.suggestions.related_pages.len(), 1);
    assert_eq!(out.suggestions.link_suggestions.len(), 1);
    let l = &out.suggestions.link_suggestions[0];
    assert_eq!((l.block_uuid.as_str(), l.start, l.end), ("b1", 7, 17));
    assert_eq!(out.dropped_links, 3);
    assert_eq!(out.suggestions.tag_suggestions[0].tag, "devops");
    assert_eq!(runs.lock().unwrap()[0]["_agent"], "bitacora-recommender");

    // An excluded page is never sent, and neither is anything without consent.
    let (agui2, runs2) = serve(answer("{}")).await;
    let deps2 = RecommendDeps::new(agui2, deps.lookup.clone(), guard(true));
    let err = run_recommend(
        &deps2,
        &RecommendRequest {
            page: "Secrets".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AgentError::Unavailable(_)));
    let deps3 = RecommendDeps::new(deps2.agui.clone(), deps.lookup.clone(), guard(false));
    assert!(run_recommend(&deps3, &req).await.is_err());
    assert!(runs2.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_run_times_out_and_is_cancelled() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancels: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = cancels.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16 * 1024];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let first = String::from_utf8_lossy(&buf[..n])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                if first.contains("/cancel") {
                    seen.lock().unwrap().push(first);
                    let _ = sock
                        .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                }
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
                let _ = sock.write_all(head.as_bytes()).await;
                tokio::time::sleep(Duration::from_secs(30)).await;
            });
        }
    });
    let agui = PandoClient::new(PandoConfig::new(format!("http://{addr}")).with_token("t"))
        .unwrap()
        .agui();
    let mut deps = ReviewDeps::new(agui, lookup(), guard(true), "g");
    deps.timeout = Duration::from_millis(300);
    let err = run_review(&deps, ReviewRange::day(DAY), false)
        .await
        .unwrap_err();
    assert!(matches!(err, AgentError::Timeout), "{err}");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(cancels.lock().unwrap().len(), 1);
}
