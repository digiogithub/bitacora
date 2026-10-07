#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Mock-server tests for the AG-UI client: a real axum server (and, for chunk control, a raw TCP
//! server) on an ephemeral port plays Pando's adapter.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use pando::agui::{
    AguiOptions, Event, Interrupt, Message, RunInput, RunOutcome, Thread, hitl, role,
};
use pando::{Error, PandoClient, PandoConfig};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct Seen {
    /// (route, authorization header, body)
    calls: Vec<(String, Option<String>, Value)>,
    /// How many POST runs were served (selects the scripted answer).
    runs: usize,
}

type Shared = Arc<Mutex<Seen>>;

fn record(state: &Shared, route: &str, headers: &HeaderMap, body: Value) -> usize {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut seen = state.lock().unwrap();
    seen.calls.push((route.to_owned(), auth, body));
    seen.runs += 1;
    seen.runs
}

fn sse(frames: &[Value]) -> Response {
    let mut body = String::from(": keep-alive\n\n");
    for f in frames {
        body.push_str(&format!("data: {f}\n\n"));
    }
    ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response()
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn client(base: &str) -> PandoClient {
    PandoClient::new(PandoConfig::new(base).with_token("secret")).expect("client")
}

async fn collect(mut stream: pando::agui::RunStream) -> Vec<Event> {
    let mut out = Vec::new();
    while let Some(e) = stream.next().await {
        out.push(e.expect("event"));
    }
    out
}

fn app(state: Shared) -> Router {
    Router::new()
        .route(
            "/api/v1/agui/info",
            get(|| async {
                axum::Json(json!({
                    "protocol": "ag-ui", "version": "1", "path": "/api/v1/agui",
                    "agents": [{"name": "coder", "url": "http://x/api/v1/agui/coder",
                                "model": {"id": "m1", "provider": "p"}, "futureField": 1}],
                    "capabilities": {"frontendTools": true, "humanInTheLoop": true,
                                     "sharedState": true, "interrupts": true}
                }))
            }),
        )
        .route(
            "/api/v1/agui/healthz",
            get(|| async {
                axum::Json(json!({"status": "ok", "version": "9", "uptimeSeconds": 1.5,
                                  "activeRuns": 2, "maxConcurrentRuns": 4, "draining": false}))
            }),
        )
        .route(
            "/api/v1/agui/{agent}",
            post(
                |State(s): State<Shared>,
                 Path(agent): Path<String>,
                 h: HeaderMap,
                 axum::Json(b): axum::Json<Value>| async move {
                    let n = record(&s, &format!("run:{agent}"), &h, b.clone());
                    if h.get("Authorization").and_then(|v| v.to_str().ok()) != Some("Bearer secret")
                    {
                        return (StatusCode::UNAUTHORIZED, axum::Json(json!({"error": "no"})))
                            .into_response();
                    }
                    let thread = b["threadId"].clone();
                    match (agent.as_str(), n) {
                        ("html", _) => ([(header::CONTENT_TYPE, "text/html")], "<html>").into_response(),
                        ("busy", _) => (
                            StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(json!({"error": "over capacity"})),
                        )
                            .into_response(),
                        ("failing", _) => sse(&[
                            json!({"type": "RUN_STARTED", "threadId": thread, "runId": "r"}),
                            json!({"type": "RUN_ERROR", "message": "boom", "code": "session_busy"}),
                        ]),
                        // First run of the HITL script: stream text, then a permission prompt.
                        ("coder", 1) => sse(&[
                            json!({"type": "RUN_STARTED", "threadId": thread, "runId": "r1"}),
                            json!({"type": "STATE_SNAPSHOT", "snapshot": {"todos": []}}),
                            json!({"type": "FUTURE_EVENT", "x": 1}),
                            json!({"type": "TEXT_MESSAGE_START", "messageId": "m1", "role": "assistant"}),
                            json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m1", "delta": "Hel"}),
                            json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m1", "delta": "lo"}),
                            json!({"type": "TEXT_MESSAGE_END", "messageId": "m1"}),
                            json!({"type": "STATE_DELTA", "delta": [
                                {"op": "add", "path": "/todos/-", "value": {"id": 1}}]}),
                            json!({"type": "TOOL_CALL_START", "toolCallId": "perm-1",
                                   "toolCallName": "pando_permission_request"}),
                            json!({"type": "TOOL_CALL_ARGS", "toolCallId": "perm-1",
                                   "delta": "{\"toolName\":\"bash\","}),
                            json!({"type": "TOOL_CALL_ARGS", "toolCallId": "perm-1",
                                   "delta": "\"action\":\"execute\"}"}),
                            json!({"type": "TOOL_CALL_END", "toolCallId": "perm-1"}),
                            json!({"type": "CUSTOM", "name": "pando.summarize", "value": 1}),
                            json!({"type": "RUN_FINISHED", "threadId": thread, "runId": "r1",
                                   "outcome": "interrupt"}),
                        ]),
                        // Second run resumes after the answer.
                        _ => sse(&[
                            json!({"type": "RUN_STARTED", "threadId": thread, "runId": "r2"}),
                            json!({"type": "TEXT_MESSAGE_START", "messageId": "m2", "role": "assistant"}),
                            json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m2", "delta": "done"}),
                            json!({"type": "RUN_FINISHED", "threadId": thread, "runId": "r2",
                                   "outcome": "success"}),
                        ]),
                    }
                },
            ),
        )
        .route(
            "/api/v1/agui/threads",
            get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
                axum::Json(json!({
                    "threads": [{"threadId": "t1", "sessionId": "s1", "agent": "coder",
                                 "updatedAt": "2026-10-07 10:00:00"}],
                    "limit": q["limit"].parse::<u32>().unwrap(),
                    "offset": q["offset"].parse::<u32>().unwrap(),
                    "hasMore": true
                }))
            }),
        )
        .route(
            "/api/v1/agui/threads/{id}/messages",
            get(|Path(id): Path<String>| async move {
                if id == "gone" {
                    return (StatusCode::NOT_FOUND, axum::Json(json!({"error": "thread not found"})))
                        .into_response();
                }
                axum::Json(json!({"threadId": id, "messages": [
                    {"id": "a", "role": "user", "content": "hi"},
                    {"id": "b", "role": "assistant", "content": "yo",
                     "toolCalls": [{"id": "c", "function": {"name": "n", "arguments": "{}"}}]}
                ]}))
                .into_response()
            }),
        )
        .route(
            "/api/v1/agui/threads/{id}/stream",
            get(|Path(id): Path<String>| async move {
                if id == "idle" {
                    return (StatusCode::NOT_FOUND, axum::Json(json!({"error": "no live run"})))
                        .into_response();
                }
                sse(&[json!({"type": "RUN_STARTED", "threadId": id, "runId": "r"}),
                      json!({"type": "RUN_FINISHED", "threadId": id, "runId": "r"})])
            }),
        )
        .route(
            "/api/v1/agui/threads/{id}",
            delete(|State(s): State<Shared>, h: HeaderMap, Path(id): Path<String>| async move {
                record(&s, "delete", &h, json!(id));
                StatusCode::NO_CONTENT
            }),
        )
        .route(
            "/api/v1/agui/runs/{id}/cancel",
            post(|State(s): State<Shared>, h: HeaderMap, Path(id): Path<String>| async move {
                record(&s, "cancel", &h, json!(id));
                StatusCode::NO_CONTENT
            }),
        )
        .with_state(state)
}

#[tokio::test]
async fn info_and_thread_api() {
    let state = Shared::default();
    let base = serve(app(state.clone())).await;
    let agui = client(&base).agui();

    let info = agui.info().await.unwrap();
    assert_eq!(info.agents[0].name, "coder");
    assert_eq!(info.agents[0].model.as_ref().unwrap().id, "m1");
    assert!(info.capabilities.interrupts && info.capabilities.human_in_the_loop);

    let health = agui.healthz().await.unwrap();
    assert_eq!(
        (
            health.status.as_str(),
            health.active_runs,
            health.max_concurrent_runs
        ),
        ("ok", 2, 4)
    );
    assert!(!health.draining);

    let page = agui.list_threads(20, 40).await.unwrap();
    assert_eq!((page.limit, page.offset, page.has_more), (20, 40, true));
    assert_eq!(page.threads[0].thread_id, "t1");

    let msgs = agui.thread_messages("t1").await.unwrap().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].tool_calls[0].function.name, "n");
    assert!(agui.thread_messages("gone").await.unwrap().is_none());

    agui.delete_thread("t 1/x").await.unwrap();
    agui.cancel_run("t1").await.unwrap();
    let seen = state.lock().unwrap();
    // Path segments are percent-encoded server-side-safe; the mock decodes them back.
    assert_eq!(seen.calls[0].0, "delete");
    assert_eq!(seen.calls[0].1.as_deref(), Some("Bearer secret"));
    assert_eq!(seen.calls[0].2, json!("t 1/x"));
    assert_eq!(seen.calls[1].0, "cancel");
}

#[tokio::test]
async fn attach_streams_or_reports_no_live_run() {
    let base = serve(app(Shared::default())).await;
    let agui = client(&base).agui();
    assert!(agui.attach("idle").await.unwrap().is_none());
    let events = collect(agui.attach("live").await.unwrap().unwrap()).await;
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[1],
        Event::RunFinished { outcome: None, .. }
    ));
}

#[tokio::test]
async fn run_text_sends_bearer_and_concatenates_deltas() {
    let state = Shared::default();
    let base = serve(app(state.clone())).await;
    let text = client(&base).agui().run_text("hello").await.unwrap();
    assert_eq!(text, "Hello");

    let seen = state.lock().unwrap();
    let (route, auth, body) = &seen.calls[0];
    assert_eq!(route, "run:coder");
    assert_eq!(auth.as_deref(), Some("Bearer secret"));
    assert!(body["threadId"].as_str().unwrap().starts_with("thread-"));
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "hello");
}

#[tokio::test]
async fn errors_map_to_typed_variants() {
    let base = serve(app(Shared::default())).await;
    let agui = client(&base).agui();
    let input = RunInput::new().with_prompt("x");

    assert!(matches!(
        agui.run_agent("html", &input).await,
        Err(Error::Protocol(_))
    ));
    assert!(matches!(
        agui.run_agent("busy", &input).await,
        Err(Error::Server { status: 503, ref message }) if message == "over capacity"
    ));
    let wrong = PandoClient::new(PandoConfig::new(&base).with_token("nope")).unwrap();
    assert!(matches!(
        wrong.agui().run(&input).await,
        Err(Error::Unauthorized)
    ));

    let agui = client(&base).agui_with(AguiOptions::default().with_agent("failing"));
    match agui.run_text("x").await {
        Err(Error::Run { code, message }) => {
            assert_eq!(code.as_deref(), Some("session_busy"));
            assert_eq!(message, "boom");
        }
        other => panic!("expected run error, got {other:?}"),
    }
}

#[tokio::test]
async fn token_override_and_dedicated_base_url() {
    let state = Shared::default();
    let base = serve(app(state.clone())).await;
    // The REST client points nowhere; the adapter options carry both URL and token.
    let rest = PandoClient::new(PandoConfig::new("http://127.0.0.1:1").with_token("rest")).unwrap();
    let agui = rest.agui_with(
        AguiOptions::default()
            .with_base_url(&base)
            .with_token("secret"),
    );
    assert_eq!(agui.run_text("hi").await.unwrap(), "Hello");
}

#[tokio::test]
async fn thread_tracks_interrupt_and_resumes_with_transcript() {
    let state = Shared::default();
    let base = serve(app(state.clone())).await;
    let agui = client(&base).agui();
    let mut thread = Thread::with_id(agui, "thread-1");

    let outcome = thread
        .send("run something")
        .await
        .unwrap()
        .drain()
        .await
        .unwrap();
    assert_eq!(outcome, RunOutcome::Interrupted);
    assert!(thread.is_interrupted());

    // Transcript: user + assistant text with the permission call attached to it.
    assert_eq!(thread.messages[0].role, role::USER);
    assert_eq!(thread.messages[1].content.text(), "Hello");
    assert_eq!(thread.messages[1].tool_calls[0].id, "perm-1");
    // State: snapshot then delta; unknown event and custom event did not break anything.
    assert_eq!(
        thread.state.as_ref().unwrap(),
        &json!({"todos": [{"id": 1}]})
    );
    assert_eq!(thread.custom_events[0].0, "pando.summarize");

    let interrupts = thread.interrupts();
    let Interrupt::Permission {
        tool_call_id,
        request,
    } = &interrupts[0]
    else {
        panic!("expected a permission interrupt, got {interrupts:?}");
    };
    assert_eq!(request.tool_name, "bash");
    assert_eq!(request.action, "execute");

    let id = tool_call_id.clone();
    let outcome = thread
        .resume(&id, hitl::approve())
        .await
        .unwrap()
        .drain()
        .await
        .unwrap();
    assert_eq!(outcome, RunOutcome::Finished);
    assert!(!thread.is_interrupted());
    assert!(thread.pending_tool_calls().is_empty());
    assert_eq!(thread.messages.last().unwrap().content.text(), "done");

    // The resumed run resent the whole transcript ending with the tool answer.
    let seen = state.lock().unwrap();
    let body = &seen.calls[1].2;
    assert_eq!(body["threadId"], "thread-1");
    let messages = body["messages"].as_array().unwrap();
    let last = messages.last().unwrap();
    assert_eq!(last["role"], "tool");
    assert_eq!(last["toolCallId"], "perm-1");
    assert_eq!(last["content"], r#"{"approved":true}"#);
    assert_eq!(messages[0]["content"], "run something");
}

#[tokio::test]
async fn failed_start_does_not_pollute_the_transcript() {
    let base = serve(app(Shared::default())).await;
    let wrong = PandoClient::new(PandoConfig::new(&base).with_token("nope")).unwrap();
    let mut thread = Thread::new(wrong.agui());
    assert!(matches!(thread.send("hi").await, Err(Error::Unauthorized)));
    assert!(thread.messages.is_empty());
}

#[tokio::test]
async fn tool_error_message_serializes_as_error() {
    let m = Message::tool_error("c1", "user declined");
    let v = serde_json::to_value(&m).unwrap();
    assert_eq!(v["role"], "tool");
    assert_eq!(v["toolCallId"], "c1");
    assert_eq!(v["error"], "user declined");
    assert!(v.get("content").is_none());
}

/// Raw TCP server: writes an SSE response in awkward chunks, then optionally stalls.
async fn raw_sse_server(chunks: Vec<Vec<u8>>, stall: bool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let Ok((mut sock, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let _ = sock.read(&mut buf).await;
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
        let _ = sock.write_all(head.as_bytes()).await;
        for chunk in chunks {
            let framed = [
                format!("{:x}\r\n", chunk.len()).into_bytes(),
                chunk,
                b"\r\n".to_vec(),
            ]
            .concat();
            let _ = sock.write_all(&framed).await;
            let _ = sock.flush().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if stall {
            tokio::time::sleep(Duration::from_secs(30)).await;
        } else {
            let _ = sock.write_all(b"0\r\n\r\n").await;
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn events_split_across_network_chunks_are_reassembled() {
    let chunks = vec![
        b"data: {\"type\":\"RUN_STAR".to_vec(),
        b"TED\",\"threadId\":\"t\",\"runId\":\"r\"}\n".to_vec(),
        b"\n: keep-alive\n\ndata: {\"type\":\"TEXT_MESSAGE_CONTENT\",\"messageId\":\"m\",\"delta\":\"h\xc3".to_vec(),
        b"\xa9\"}\r\n\r\ndata: not json\n\ndata: {\"type\":\"RUN_FINISHED\",\"threadId\":\"t\",\"runId\":\"r\"}".to_vec(),
    ];
    let base = raw_sse_server(chunks, false).await;
    let stream = client(&base)
        .agui()
        .run(&RunInput::new().with_prompt("x"))
        .await
        .unwrap();
    let events = collect(stream).await;
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(matches!(&events[1], Event::TextMessageContent { delta, .. } if delta == "hé"));
    assert!(matches!(&events[2], Event::RunFinished { .. }));
}

#[tokio::test]
async fn stalled_stream_hits_the_idle_timeout() {
    let chunks = vec![b"data: {\"type\":\"RUN_STARTED\"}\n\n".to_vec()];
    let base = raw_sse_server(chunks, true).await;
    let cfg = PandoConfig::new(&base).with_stream_idle_timeout(Duration::from_millis(300));
    let agui = PandoClient::new(cfg).unwrap().agui();
    let mut stream = agui.run(&RunInput::new().with_prompt("x")).await.unwrap();
    assert!(matches!(
        stream.next().await,
        Some(Ok(Event::RunStarted { .. }))
    ));
    assert!(matches!(
        stream.next().await,
        Some(Err(Error::Timeout | Error::Unreachable(_)))
    ));
    assert!(stream.next().await.is_none());
}
