#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Compose and continue runs against an axum SSE mock (BIT-US-0153): the preview streams, the
//! answer is cleaned, and nothing reaches the server for excluded or private pages.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use bitacora_config::pando::GraphConsent;
use bitacora_pando::agents::compose::{ComposeMode, ComposeRequest, PageLookup, PageMeta};
use bitacora_pando::agents::{AgentError, ComposeDeps, ContentGuard, run_compose};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

#[derive(Clone)]
struct Mock {
    runs: Arc<Mutex<Vec<Value>>>,
    answer: String,
}

fn sse(frames: &[Value]) -> Response {
    let mut body = String::new();
    for f in frames {
        body.push_str(&format!("data: {f}\n\n"));
    }
    ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response()
}

async fn serve(answer: &str) -> (pando::agui::AguiClient, Arc<Mutex<Vec<Value>>>) {
    let mock = Mock {
        runs: Arc::default(),
        answer: answer.to_owned(),
    };
    let runs = mock.runs.clone();
    let app = Router::new()
        .route(
            "/api/v1/agui/{agent}",
            post(
                |State(m): State<Mock>,
                 Path(agent): Path<String>,
                 axum::Json(mut b): axum::Json<Value>| async move {
                    let t = b["threadId"].clone();
                    b["_agent"] = json!(agent);
                    m.runs.lock().unwrap().push(b);
                    let (head, tail) = m.answer.split_at(m.answer.len() / 2);
                    sse(&[
                        json!({"type": "RUN_STARTED", "threadId": t, "runId": "r"}),
                        json!({"type": "TEXT_MESSAGE_START", "messageId": "m", "role": "assistant"}),
                        json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m", "delta": head}),
                        json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": "m", "delta": tail}),
                        json!({"type": "TEXT_MESSAGE_END", "messageId": "m"}),
                        json!({"type": "RUN_FINISHED", "threadId": t, "runId": "r", "outcome": "success"}),
                    ])
                },
            ),
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

struct Pages;

impl PageLookup for Pages {
    fn page(&self, title: &str) -> Option<PageMeta> {
        Some(PageMeta {
            file_path: format!("pages/{title}.md"),
            tags: if title == "Tagged" {
                vec!["secrets".into()]
            } else {
                Vec::new()
            },
            private: title == "Diary",
        })
    }
}

fn deps(client: pando::agui::AguiClient, granted: bool) -> ComposeDeps {
    let guard = ContentGuard::from_consent(&GraphConsent {
        granted,
        exclusions: vec!["Secrets".into(), "Hidden".into()],
        ..GraphConsent::default()
    });
    ComposeDeps::new(client, guard, Arc::new(Pages))
}

fn req(page: &str, mode: ComposeMode) -> ComposeRequest {
    ComposeRequest {
        mode,
        page: page.into(),
        instruction: "tighten this".into(),
        block_text: "a rough draft".into(),
        include_block: true,
    }
}

#[tokio::test]
async fn streams_the_preview_and_returns_the_cleaned_answer() {
    let (client, runs) = serve("\"A tighter draft.\"").await;
    let d = deps(client, true);
    let mut previews = Vec::new();
    let out = run_compose(&d, &req("Notes", ComposeMode::Compose), |p| {
        previews.push(p.to_owned());
    })
    .await
    .unwrap();
    assert_eq!(out, "A tighter draft.");
    assert_eq!(previews.len(), 2, "{previews:?}");
    assert!(previews[1].starts_with(&previews[0]));
    let sent = runs.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["_agent"], "bitacora-writer");
    assert!(
        sent[0]["context"].to_string().contains("a rough draft"),
        "the block is attached as context: {}",
        sent[0]
    );
}

#[tokio::test]
async fn the_block_is_not_sent_unless_the_chip_is_on() {
    let (client, runs) = serve("ok").await;
    let d = deps(client, true);
    let mut r = req("Notes", ComposeMode::Compose);
    r.include_block = false;
    run_compose(&d, &r, |_| {}).await.unwrap();
    let sent = runs.lock().unwrap();
    assert!(!sent[0].to_string().contains("a rough draft"));
}

#[tokio::test]
async fn nothing_is_sent_for_excluded_private_or_unconsented_pages() {
    let (client, runs) = serve("never").await;
    for (page, granted) in [
        ("Secrets", true),
        ("Secrets/child", true),
        ("Tagged", true),
        ("Diary", true),
        ("Notes", false),
    ] {
        let d = deps(client.clone(), granted);
        for mode in [ComposeMode::Compose, ComposeMode::Continue] {
            let err = run_compose(&d, &req(page, mode), |_| {})
                .await
                .expect_err(page);
            assert!(matches!(err, AgentError::Unavailable(_)), "{page}: {err}");
        }
    }
    assert!(runs.lock().unwrap().is_empty(), "no request was made");
}

#[tokio::test]
async fn a_private_block_is_not_sent() {
    let (client, runs) = serve("never").await;
    let d = deps(client, true);
    let mut r = req("Notes", ComposeMode::Continue);
    r.block_text = "secret\nprivate:: true".into();
    assert!(run_compose(&d, &r, |_| {}).await.is_err());
    assert!(runs.lock().unwrap().is_empty());
}
