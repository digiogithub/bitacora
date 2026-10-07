#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Hybrid search over a real index against a mock Pando KB (BIT-US-0144, BIT-SP-0010.R4/R5).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use bitacora_config::EffectiveConfig;
use bitacora_index::{Index, IndexLocation, Indexer, IndexerOptions, OpenOptions, graph_id};
use bitacora_pando::semantic::{
    ContentPolicy, Gate, GraphInfo, HybridOptions, HybridSearch, HybridTarget, IndexSource,
    KbProvider, Remote, SemanticState, SharedPolicy, Unavailable, doc_id,
};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

const U_FERRY: &str = "6650a1b2-0000-4000-8000-000000000001";
const U_BOAT: &str = "6650a1b2-0000-4000-8000-000000000002";
const U_SECRET: &str = "6650a1b2-0000-4000-8000-000000000003";
const U_GONE: &str = "6650a1b2-0000-4000-8000-0000000000ff";

const TRIP: &str = "- Book the ferry to the island before noon\n  id:: 6650a1b2-0000-4000-8000-000000000001\n\
- Sailing boats are lovely in the summer season\n  id:: 6650a1b2-0000-4000-8000-000000000002\n";
const WORK: &str =
    "- Confidential ferry budget for the quarter\n  id:: 6650a1b2-0000-4000-8000-000000000003\n";

struct Env {
    _tmp: tempfile::TempDir,
    _index: Index,
    _indexer: Indexer,
    graph_id: String,
    policy: SharedPolicy,
    search: HybridSearch,
}

#[derive(Default)]
struct Mock {
    hits: Mutex<Vec<Value>>,
    requests: Mutex<Vec<Value>>,
    fail: AtomicBool,
    delay: Mutex<Duration>,
}

async fn serve(mock: Arc<Mock>) -> String {
    let app = Router::new()
        .route(
            "/api/v1/remembrances/kb/search",
            post(
                |State(m): State<Arc<Mock>>, axum::Json(b): axum::Json<Value>| async move {
                    m.requests.lock().unwrap().push(b);
                    let delay = *m.delay.lock().unwrap();
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    if m.fail.load(Ordering::SeqCst) {
                        return (StatusCode::SERVICE_UNAVAILABLE, "down").into_response();
                    }
                    let results = m.hits.lock().unwrap().clone();
                    axum::Json(json!({ "results": results })).into_response()
                },
            ),
        )
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn env(base: Option<String>, gate_open: bool) -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let graph = tmp.path().join("graph");
    write(&graph, "pages/Trip.md", TRIP);
    write(&graph, "pages/work/Budget.md", WORK);
    let cfg = EffectiveConfig::default();
    let loc = IndexLocation::in_data_dir(&tmp.path().join("data"), &graph).unwrap();
    let index = Index::open(loc, OpenOptions::for_config(&graph, &cfg)).unwrap();
    let indexer = Indexer::start(&index, IndexerOptions::new(&graph, cfg)).unwrap();
    indexer.reconcile().unwrap();
    let id = graph_id(&graph).unwrap();
    let policy: SharedPolicy = Arc::new(parking_lot::RwLock::new(ContentPolicy::default()));
    let reader = index.read_api();
    let info = GraphInfo {
        id: id.clone(),
        name: "notes".into(),
    };
    let source = Arc::new(IndexSource::new(reader.clone(), info, Arc::clone(&policy)));
    let remote = base.map(|b| {
        let client = PandoClient::new(PandoConfig::new(b)).expect("client");
        let kb: KbProvider = Arc::new(move || Some(client.kb()));
        let gate: Gate = Arc::new(move || gate_open);
        Remote {
            handle: tokio::runtime::Handle::current(),
            kb,
            gate,
        }
    });
    let search = HybridSearch::new(reader, source, id.clone(), remote);
    Env {
        _tmp: tmp,
        _index: index,
        _indexer: indexer,
        graph_id: id,
        policy,
        search,
    }
}

fn hit(graph: &str, uuid: &str) -> Value {
    json!({ "file_path": doc_id(graph, uuid), "chunk_content": "x", "score": 0.9 })
}

fn uuids(r: &bitacora_pando::semantic::HybridResults) -> Vec<String> {
    r.hits
        .iter()
        .filter_map(|h| match &h.target {
            HybridTarget::Block { uuid } => Some(uuid.clone()),
            HybridTarget::Page { .. } => None,
        })
        .collect()
}

/// `HybridSearch::search` blocks, so it runs off the async worker threads.
async fn run(e: &Env, q: &str, opts: HybridOptions) -> bitacora_pando::semantic::HybridResults {
    let s = e.search.clone();
    let q = q.to_owned();
    tokio::task::spawn_blocking(move || s.search(&q, &opts).expect("search"))
        .await
        .expect("join")
}

#[tokio::test(flavor = "multi_thread")]
async fn fuses_scopes_to_the_graph_and_re_resolves_hits() {
    let mock = Arc::new(Mock::default());
    let base = serve(Arc::clone(&mock)).await;
    let e = env(Some(base), true);
    *mock.hits.lock().unwrap() = vec![
        hit(&e.graph_id, U_BOAT),                          // semantic only
        hit(&e.graph_id, U_FERRY),                         // also a lexical match
        hit(&e.graph_id, U_GONE),                          // block no longer in the index
        hit(&e.graph_id, U_SECRET),                        // excluded below
        json!({ "file_path": "other/doc", "score": 0.1 }), // not ours
    ];
    e.policy.write().exclusions = vec!["pages/work/".into()];
    let r = run(&e, "ferry", HybridOptions::default()).await;

    assert_eq!(
        r.semantic,
        SemanticState::Used {
            candidates: 5,
            dropped: 3
        }
    );
    let ids = uuids(&r);
    assert!(ids.contains(&U_FERRY.to_owned()) && ids.contains(&U_BOAT.to_owned()));
    assert!(!ids.contains(&U_GONE.to_owned()));
    // Local FTS still finds the excluded page (exclusions only gate what leaves the machine),
    // but its semantic hit was dropped.
    let secret = r
        .hits
        .iter()
        .find(|h| {
            h.target
                == HybridTarget::Block {
                    uuid: U_SECRET.into(),
                }
        })
        .unwrap();
    assert_eq!(secret.semantic_rank, None);
    // ferry is in both lists, so it outranks the semantic-only boat.
    assert_eq!(ids[0], U_FERRY);
    let ferry = &r.hits[0];
    assert!(ferry.lexical_rank.is_some() && ferry.semantic_rank == Some(2));
    let boat = r
        .hits
        .iter()
        .find(|h| {
            h.target
                == HybridTarget::Block {
                    uuid: U_BOAT.into(),
                }
        })
        .unwrap();
    assert_eq!((boat.lexical_rank, boat.semantic_rank), (None, Some(1)));
    assert!(boat.snippet.text.contains("Sailing boats"));
    assert_eq!(boat.title, "Trip");

    // The request is scoped to this graph's documents.
    let req = mock.requests.lock().unwrap()[0].clone();
    assert_eq!(req["path_prefix"], format!("bitacora/{}/", e.graph_id));
    assert_eq!(req["query"], "ferry");
}

#[tokio::test(flavor = "multi_thread")]
async fn falls_back_to_lexical_when_pando_fails() {
    let mock = Arc::new(Mock::default());
    mock.fail.store(true, Ordering::SeqCst);
    let e = env(Some(serve(Arc::clone(&mock)).await), true);
    let r = run(&e, "ferry", HybridOptions::default()).await;
    assert!(matches!(
        r.semantic,
        SemanticState::Unavailable(Unavailable::Failed(_))
    ));
    assert!(uuids(&r).contains(&U_FERRY.to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slow_pando_times_out_without_losing_lexical_results() {
    let mock = Arc::new(Mock::default());
    *mock.delay.lock().unwrap() = Duration::from_secs(3);
    let e = env(Some(serve(Arc::clone(&mock)).await), true);
    let opts = HybridOptions {
        timeout: Duration::from_millis(150),
        ..HybridOptions::default()
    };
    let started = std::time::Instant::now();
    let r = run(&e, "ferry", opts).await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(r.semantic, SemanticState::Unavailable(Unavailable::Timeout));
    assert!(uuids(&r).contains(&U_FERRY.to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_gate_and_missing_remote_are_lexical_only_without_requests() {
    let mock = Arc::new(Mock::default());
    let base = serve(Arc::clone(&mock)).await;
    let closed = env(Some(base), false);
    let r = run(&closed, "ferry", HybridOptions::default()).await;
    assert_eq!(r.semantic, SemanticState::Unavailable(Unavailable::Offline));
    assert!(uuids(&r).contains(&U_FERRY.to_owned()));
    assert!(mock.requests.lock().unwrap().is_empty());

    let none = env(None, true);
    let r = run(&none, "ferry", HybridOptions::default()).await;
    assert_eq!(
        r.semantic,
        SemanticState::Unavailable(Unavailable::Disabled)
    );
    assert!(uuids(&r).contains(&U_FERRY.to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn blank_query_returns_nothing_and_asks_nobody() {
    let mock = Arc::new(Mock::default());
    let e = env(Some(serve(Arc::clone(&mock)).await), true);
    let r = run(&e, "   ", HybridOptions::default()).await;
    assert!(r.hits.is_empty());
    assert!(mock.requests.lock().unwrap().is_empty());
}
