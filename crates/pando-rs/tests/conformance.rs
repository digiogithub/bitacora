#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Fixture-replay conformance: the SSE streams shared with Pando's other SDKs
//! (`tests/fixtures/agui/*.sse`, copied byte for byte from `sdk/typescript/tests/fixtures/agui`)
//! are served verbatim by a local server and must reduce to the same state in this SDK.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::post;
use pando::agui::{Event, RunOutcome, Thread};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

const INTERRUPT: &str = include_str!("fixtures/agui/interrupt-frontend-tool.sse");
const RESUME: &str = include_str!("fixtures/agui/resume-frontend-tool.sse");
const STATE_DELTA: &str = include_str!("fixtures/agui/state-delta-todos-tokenusage-files.sse");

#[derive(Default)]
struct Replay {
    /// Bodies served in order, one per POST.
    queue: Vec<&'static str>,
    /// Request bodies received.
    bodies: Vec<Value>,
}

type Shared = Arc<Mutex<Replay>>;

/// Serves the queued fixtures verbatim; returns a client and the shared log.
async fn replay(queue: Vec<&'static str>) -> (PandoClient, Shared) {
    let state: Shared = Arc::new(Mutex::new(Replay {
        queue,
        bodies: Vec::new(),
    }));
    let app = Router::new()
        .route(
            "/api/v1/agui/{agent}",
            post(
                |State(s): State<Shared>, axum::Json(body): axum::Json<Value>| async move {
                    let mut s = s.lock().unwrap();
                    s.bodies.push(body);
                    let n = s.bodies.len() - 1;
                    let text = s.queue.get(n).copied().unwrap_or("");
                    ([(header::CONTENT_TYPE, "text/event-stream")], text).into_response()
                },
            ),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let client = PandoClient::new(PandoConfig::new(format!("http://{addr}"))).expect("client");
    (client, state)
}

#[tokio::test]
async fn interrupt_surfaces_the_pending_frontend_tool_call() {
    let (client, _) = replay(vec![INTERRUPT]).await;
    let mut thread = Thread::with_id(client.agui(), "rec-thread-1");
    let outcome = thread
        .send("Call the get_weather tool for Madrid.")
        .await
        .unwrap()
        .drain()
        .await
        .unwrap();
    assert_eq!(outcome, RunOutcome::Interrupted);
    assert!(thread.is_interrupted());
    let pending = thread.pending_tool_calls();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "rec-call-weather-1");
    assert_eq!(pending[0].name, "get_weather");
    assert_eq!(pending[0].args, Some(json!({"city": "Madrid"})));
    // The snapshot is folded into the state document.
    let state = thread.state.as_ref().unwrap();
    assert_eq!(state["agent"], "coder");
    assert_eq!(state["model"]["id"], "claude-x");
}

#[tokio::test]
async fn resume_posts_the_tool_result_after_the_last_user_message() {
    let (client, log) = replay(vec![INTERRUPT, RESUME]).await;
    let mut thread = Thread::with_id(client.agui(), "rec-thread-1");
    thread
        .send("Call the get_weather tool for Madrid.")
        .await
        .unwrap()
        .drain()
        .await
        .unwrap();
    let result = json!({"tempC": 22, "condition": "sunny"}).to_string();
    let outcome = thread
        .resume("rec-call-weather-1", result.clone())
        .await
        .unwrap()
        .drain()
        .await
        .unwrap();
    assert_eq!(outcome, RunOutcome::Finished);
    assert!(!thread.is_interrupted());
    assert!(thread.pending_tool_calls().is_empty());

    let body = log.lock().unwrap().bodies[1].clone();
    assert_eq!(body["threadId"], "rec-thread-1");
    let messages = body["messages"].as_array().unwrap();
    let last_user = messages.iter().rposition(|m| m["role"] == "user").unwrap();
    let trailing = &messages[last_user + 1..];
    let assistant = trailing
        .iter()
        .position(|m| {
            m["role"] == "assistant"
                && m["toolCalls"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|c| c["id"] == "rec-call-weather-1"))
        })
        .expect("assistant message carrying the tool call");
    let tool = trailing
        .iter()
        .position(|m| m["role"] == "tool" && m["toolCallId"] == "rec-call-weather-1")
        .expect("tool result message");
    assert!(assistant < tool);
    assert_eq!(trailing[tool]["content"], result);

    let last = thread
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .unwrap();
    assert!(format!("{last:?}").contains("22"));
}

#[tokio::test]
async fn state_deltas_reduce_to_todos_token_usage_and_files() {
    let (client, _) = replay(vec![STATE_DELTA]).await;
    let mut thread = Thread::with_id(client.agui(), "rec-thread-2");
    let outcome = thread
        .send("Read the README, then write a two-item todo list.")
        .await
        .unwrap()
        .drain()
        .await
        .unwrap();
    assert_eq!(outcome, RunOutcome::Finished);
    assert!(!thread.state_desynced);
    let state = thread.state.as_ref().unwrap();
    assert_eq!(state["thread"], "rec-thread-2");
    assert_eq!(state["session"], "rec-session-2");
    assert_eq!(
        state["todos"],
        json!([
            {"content": "Read the deployment guide", "status": "in_progress", "priority": "high"},
            {"content": "Summarize open questions", "status": "pending", "priority": "medium"}
        ])
    );
    assert_eq!(state["tokenUsage"]["promptTokens"], 842);
    assert_eq!(state["tokenUsage"]["completionTokens"], 156);
    assert_eq!(state["tokenUsage"]["estimated"], false);
    assert_eq!(
        state["files"],
        json!([{"path": "docs/DEPLOY.md", "name": "DEPLOY.md", "action": "read"}])
    );
}

#[tokio::test]
async fn every_fixture_event_is_a_known_event() {
    // The recorded streams only use events the SDK models; none may fall into `Unknown`.
    for fixture in [INTERRUPT, RESUME, STATE_DELTA] {
        let (client, _) = replay(vec![fixture]).await;
        let mut thread = Thread::new(client.agui());
        let mut run = thread.send("x").await.unwrap();
        let mut count = 0;
        while let Some(event) = run.next().await {
            let event = event.unwrap();
            assert!(
                !matches!(event, Event::Unknown { .. }),
                "unmodelled: {event:?}"
            );
            count += 1;
        }
        assert!(count >= 5);
    }
}
