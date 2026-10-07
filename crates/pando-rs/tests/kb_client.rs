#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Mock-server tests: a real axum server on an ephemeral port plays Pando.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use pando::kb::{EmbeddingTestKind, SearchRequest, UpsertAction, UpsertDocument};
use pando::{Error, PandoClient, PandoConfig};
use serde_json::{Value, json};

#[derive(Default)]
struct Seen {
    /// (route, token header, body)
    calls: Vec<(String, Option<String>, Value)>,
}

type Shared = Arc<Mutex<Seen>>;

fn record(state: &Shared, route: &str, headers: &HeaderMap, body: Value) {
    let token = headers
        .get("X-Pando-Token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if let Ok(mut seen) = state.lock() {
        seen.calls.push((route.to_owned(), token, body));
    }
}

fn json_resp(status: StatusCode, body: Value) -> Response {
    (status, axum::Json(body)).into_response()
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
    PandoClient::new(PandoConfig::new(base).with_token("secret-token")).expect("client")
}

fn app(state: Shared) -> Router {
    Router::new()
        .route(
            "/api/v1/remembrances/kb/documents",
            post(
                |State(s): State<Shared>, h: HeaderMap, axum::Json(b): axum::Json<Value>| async move {
                    record(&s, "upsert", &h, b.clone());
                    let action = if b["file_path"] == "exists.md" {
                        "updated"
                    } else {
                        "created"
                    };
                    json_resp(
                        StatusCode::OK,
                        json!({"file_path": b["file_path"], "action": action, "future_field": 1}),
                    )
                },
            ),
        )
        .route(
            "/api/v1/remembrances/kb/documents",
            delete(
                |State(s): State<Shared>, h: HeaderMap, axum::Json(b): axum::Json<Value>| async move {
                    record(&s, "delete", &h, b.clone());
                    json_resp(
                        StatusCode::OK,
                        json!({"file_path": b["file_path"], "status": "deleted"}),
                    )
                },
            ),
        )
        .route(
            "/api/v1/remembrances/kb/search",
            post(
                |State(s): State<Shared>, h: HeaderMap, axum::Json(b): axum::Json<Value>| async move {
                    record(&s, "search", &h, b);
                    json_resp(
                        StatusCode::OK,
                        json!({
                            "count": 1,
                            "unknown_top_level": true,
                            "results": [{
                                "file_path": "a.md", "chunk_content": "hello", "score": 0.5,
                                "rank": 1, "tags": ["t"], "created_at": "2026-01-01T00:00:00Z",
                                "updated_at": "2026-01-02T00:00:00Z", "links": 2,
                                "metadata": {"k": "v"}, "brand_new_field": [1, 2]
                            }],
                            "related_to_top_result": [
                                {"file_path": "b.md", "score": 0.1, "reasons": ["link"]}
                            ],
                            "warning": "stale"
                        }),
                    )
                },
            ),
        )
        .route(
            "/api/v1/remembrances/kb/reindex",
            post(|State(s): State<Shared>, h: HeaderMap| async move {
                record(&s, "reindex", &h, Value::Null);
                json_resp(
                    StatusCode::OK,
                    json!({"scanned": 5, "added": 1, "updated": 2, "unchanged": 1,
                           "deleted": 1, "links_indexed": 7, "extra": "x"}),
                )
            }),
        )
        .route(
            "/api/v1/remembrances/embedding-models",
            get(|State(s): State<Shared>, h: HeaderMap, uri: axum::http::Uri| async move {
                record(&s, "models", &h, json!(uri.query()));
                json_resp(
                    StatusCode::OK,
                    json!({"provider": "ollama", "source": "api",
                           "models": [{"id": "nomic", "size": "274 MB"}]}),
                )
            }),
        )
        .route(
            "/api/v1/remembrances/test-connection",
            post(
                |State(s): State<Shared>, h: HeaderMap, axum::Json(b): axum::Json<Value>| async move {
                    record(&s, "test", &h, b);
                    json_resp(
                        StatusCode::OK,
                        json!({"document": {"ok": true, "latency_ms": 12, "dimension": 768,
                                            "provider": "ollama", "model": "nomic"}}),
                    )
                },
            ),
        )
        .route(
            "/health",
            get(|| async {
                json_resp(
                    StatusCode::OK,
                    json!({"status": "healthy", "version": "1.2.3", "startup_mode": "",
                           "parent_instance_id": "", "project_id": ""}),
                )
            }),
        )
        .with_state(state)
}

async fn started() -> (PandoClient, Shared) {
    let state = Shared::default();
    let base = serve(app(state.clone())).await;
    (client(&base), state)
}

fn calls(state: &Shared) -> Vec<(String, Option<String>, Value)> {
    state.lock().map(|s| s.calls.clone()).unwrap_or_default()
}

#[tokio::test]
async fn upsert_created_and_updated_send_token_and_body() {
    let (c, state) = started().await;
    let mut doc = UpsertDocument::new("new.md", "# hi");
    doc.tags = vec!["x".into()];
    let out = c.kb().upsert(&doc).await.expect("upsert");
    assert_eq!(out.action, UpsertAction::Created);
    assert_eq!(out.file_path, "new.md");

    let out = c
        .kb()
        .upsert(&UpsertDocument::new("exists.md", "body"))
        .await
        .expect("upsert");
    assert_eq!(out.action, UpsertAction::Updated);

    let seen = calls(&state);
    assert_eq!(seen[0].1.as_deref(), Some("secret-token"));
    assert_eq!(
        seen[0].2,
        json!({"file_path": "new.md", "content": "# hi", "tags": ["x"]})
    );
    // No metadata/tags keys when unset, so the server keeps stored metadata.
    assert_eq!(
        seen[1].2,
        json!({"file_path": "exists.md", "content": "body"})
    );
}

#[tokio::test]
async fn delete_sends_json_body() {
    let (c, state) = started().await;
    c.kb().delete("gone.md").await.expect("delete");
    assert_eq!(calls(&state)[0].2, json!({"file_path": "gone.md"}));
}

#[tokio::test]
async fn search_serialises_filters_and_tolerates_unknown_fields() {
    let (c, state) = started().await;
    let mut req = SearchRequest::new("hello", 5);
    req.tags = vec!["t".into()];
    req.path_prefix = "pages/".into();
    req.scope = "docs".into();
    req.sort_by_date = true;
    req.exclude_outdated = Some(false);
    let resp = c.kb().search(&req).await.expect("search");
    assert_eq!(resp.count, 1);
    assert_eq!(resp.results[0].file_path, "a.md");
    assert_eq!(resp.results[0].links, 2);
    assert_eq!(resp.results[0].backlinks, 0);
    assert_eq!(resp.results[0].metadata["k"], "v");
    assert_eq!(resp.related_to_top_result[0].file_path, "b.md");
    assert_eq!(resp.warning.as_deref(), Some("stale"));
    assert_eq!(
        calls(&state)[0].2,
        json!({"query": "hello", "limit": 5, "tags": ["t"], "path_prefix": "pages/",
               "scope": "docs", "sort_by_date": true, "exclude_outdated": false})
    );
}

#[tokio::test]
async fn reindex_returns_stats() {
    let (c, _) = started().await;
    let stats = c.kb().reindex().await.expect("reindex");
    assert_eq!((stats.scanned, stats.added, stats.deleted), (5, 1, 1));
    assert_eq!(stats.links_indexed, 7);
}

#[tokio::test]
async fn embedding_models_and_test_connection() {
    let (c, state) = started().await;
    let models = c
        .kb()
        .embedding_models(Some("ollama"))
        .await
        .expect("models");
    assert_eq!(models.models[0].id, "nomic");
    assert_eq!(models.source, "api");
    let t = c
        .kb()
        .test_embedding(EmbeddingTestKind::Document)
        .await
        .expect("test");
    let doc = t.document.expect("document result");
    assert!(doc.ok && doc.dimension == 768);
    assert!(t.code.is_none());
    let seen = calls(&state);
    assert_eq!(seen[0].2, json!("provider=ollama"));
    assert_eq!(seen[1].2, json!({"type": "document"}));
}

#[tokio::test]
async fn info_reads_health() {
    let (c, _) = started().await;
    assert_eq!(c.info().await.expect("info").version, "1.2.3");
}

#[tokio::test]
async fn unauthorized_maps_to_error() {
    let app = Router::new().route(
        "/api/v1/remembrances/kb/search",
        post(|| async { json_resp(StatusCode::UNAUTHORIZED, json!({"error": "unauthorized"})) }),
    );
    let base = serve(app).await;
    let err = client(&base)
        .kb()
        .search(&SearchRequest::new("q", 1))
        .await
        .expect_err("401");
    assert!(matches!(err, Error::Unauthorized), "{err:?}");
}

#[tokio::test]
async fn not_configured_maps_503() {
    let app = Router::new().route(
        "/api/v1/remembrances/kb/reindex",
        post(|| async {
            json_resp(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"error": "KB filesystem mirror not configured"}),
            )
        }),
    );
    let base = serve(app).await;
    let err = client(&base).kb().reindex().await.expect_err("503");
    match err {
        Error::NotConfigured(m) => assert!(m.contains("mirror")),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn reindex_conflict_maps_to_reindex_running() {
    let app = Router::new().route(
        "/api/v1/remembrances/kb/reindex",
        post(|| async {
            json_resp(
                StatusCode::CONFLICT,
                json!({"error": "a KB reindex is already running"}),
            )
        }),
    );
    let base = serve(app).await;
    let err = client(&base).kb().reindex().await.expect_err("409");
    assert!(matches!(err, Error::ReindexRunning), "{err:?}");
}

#[tokio::test]
async fn server_error_carries_status_and_message() {
    let app = Router::new().route(
        "/api/v1/remembrances/kb/documents",
        delete(|| async { json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "boom"})) }),
    );
    let base = serve(app).await;
    let err = client(&base).kb().delete("x").await.expect_err("500");
    match err {
        Error::Server { status, message } => {
            assert_eq!((status, message.as_str()), (500, "boom"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn slow_server_times_out() {
    let app = Router::new().route(
        "/api/v1/remembrances/kb/search",
        post(|| async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            json_resp(StatusCode::OK, json!({"count": 0, "results": []}))
        }),
    );
    let base = serve(app).await;
    let c = PandoClient::new(PandoConfig::new(&base).with_timeout(Duration::from_millis(150)))
        .expect("client");
    let err = c
        .kb()
        .search(&SearchRequest::new("q", 1))
        .await
        .expect_err("timeout");
    assert!(matches!(err, Error::Timeout), "{err:?}");
}

#[tokio::test]
async fn closed_port_is_unreachable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    let err = client(&format!("http://{addr}"))
        .kb()
        .reindex()
        .await
        .expect_err("refused");
    assert!(matches!(err, Error::Unreachable(_)), "{err:?}");
}

#[test]
fn token_is_redacted_in_debug() {
    let cfg = PandoConfig::new("http://x").with_token("hunter2");
    let dbg = format!("{cfg:?} {:?}", client("http://127.0.0.1:1"));
    assert!(
        !dbg.contains("hunter2") && !dbg.contains("secret-token"),
        "{dbg}"
    );
    assert!(dbg.contains("redacted"));
}

#[test]
fn bad_base_url_is_a_config_error() {
    let err = PandoClient::new(PandoConfig::new("localhost:80")).expect_err("scheme");
    assert!(matches!(err, Error::Config(_)));
}
