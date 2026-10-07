#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Document mapping over real indexed graphs, and the worker driven by real index events
//! (BIT-US-0142, BIT-US-0143).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use bitacora_config::EffectiveConfig;
use bitacora_index::{Index, IndexLocation, Indexer, IndexerOptions, OpenOptions, graph_id};
use bitacora_pando::semantic::{
    ContentPolicy, DocSource, GraphInfo, IndexSource, Ledger, SemanticParams, SemanticWorker,
    SenderConfig, SharedPolicy,
};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

struct Built {
    _tmp: tempfile::TempDir,
    graph: std::path::PathBuf,
    index: Index,
    indexer: Indexer,
}

fn build(graph: Option<&Path>, files: &[(&str, &str)]) -> Built {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph.map_or_else(
        || {
            let g = tmp.path().join("graph");
            std::fs::create_dir_all(&g).unwrap();
            for (p, t) in files {
                write(&g, p, t);
            }
            g
        },
        Path::to_path_buf,
    );
    let data = tmp.path().join("data");
    let cfg = EffectiveConfig::default();
    let loc = IndexLocation::in_data_dir(&data, &graph).unwrap();
    let index = Index::open(loc, OpenOptions::for_config(&graph, &cfg)).unwrap();
    let indexer = Indexer::start(&index, IndexerOptions::new(&graph, cfg)).unwrap();
    Built {
        _tmp: tmp,
        graph,
        index,
        indexer,
    }
}

fn source(b: &Built, policy: ContentPolicy) -> (IndexSource, GraphInfo) {
    let info = GraphInfo {
        id: graph_id(&b.graph).unwrap(),
        name: "notes".into(),
    };
    let shared: SharedPolicy = Arc::new(parking_lot::RwLock::new(policy));
    (
        IndexSource::new(b.index.read_api(), info.clone(), shared),
        info,
    )
}

const TRIP: &str = "tags:: travel\n\n\
- Itinerary\n  - Day one #flights\n    - Book the ferry to the island before noon\n      id:: 6650a1b2-0000-4000-8000-00000000000c\n\
  - collapsed:: true\n\
- ![map](../assets/map_1.png)\n\
- TODO call mom\n\
- {{video https://example.com/v.mp4}}\n\
- A long enough thought about packing light for the trip\n  id:: 6650a1b2-0000-4000-8000-00000000000d\n";

const HEALTH: &str = "private:: true\n\n- Blood pressure readings from the morning check\n";

const JOURNAL: &str = "- Went for a long walk along the beach with friends\n  id:: 6650a1b2-0000-4000-8000-00000000000e\n- ok\n";

#[test]
fn golden_documents_of_a_small_graph() {
    let b = build(
        None,
        &[
            ("pages/Trip Plan.md", TRIP),
            ("pages/Health.md", HEALTH),
            ("journals/2024_05_01.md", JOURNAL),
        ],
    );
    b.indexer.reconcile().unwrap();
    let (src, _) = source(&b, ContentPolicy::default());
    let mut out = String::new();
    for path in src.file_paths().unwrap() {
        for d in src.file_docs(&path).unwrap() {
            let mut meta = d.metadata.clone();
            meta.remove("graph"); // path-derived, differs per run
            meta.remove("content_hash");
            out.push_str(&format!(
                "=== {} :: {}\n{}--- metadata\n{}\n--- tags {:?}\n\n",
                path,
                d.doc_id.rsplit('/').next().unwrap(),
                d.text,
                serde_json::to_string_pretty(&Value::Object(meta)).unwrap(),
                d.tags
            ));
        }
    }
    insta::assert_snapshot!(out);
}

#[test]
fn eligible_blocks_and_hashes_are_stable_across_reindexing() {
    let b = build(None, &[("pages/Trip Plan.md", TRIP)]);
    b.indexer.reconcile().unwrap();
    let (src, info) = source(&b, ContentPolicy::default());
    let first = src.file_docs("pages/Trip Plan.md").unwrap();
    assert!(!first.is_empty());
    assert!(
        first
            .iter()
            .all(|d| d.doc_id.starts_with(&format!("bitacora/{}/", info.id)))
    );
    // A second reconcile with nothing changed leaves uuids and hashes alone.
    b.indexer.reconcile().unwrap();
    let again = src.file_docs("pages/Trip Plan.md").unwrap();
    let key = |v: &[bitacora_pando::semantic::SemanticDoc]| -> Vec<(String, String)> {
        v.iter()
            .map(|d| (d.doc_id.clone(), d.content_hash.clone()))
            .collect()
    };
    assert_eq!(key(&first), key(&again));
    // Single-block lookup agrees with the per-file mapping.
    let one = &first[0];
    assert_eq!(src.block_doc(&one.block_uuid).unwrap().as_ref(), Some(one));
}

#[test]
fn fixture_graphs_map_without_collisions() {
    for name in ["edge-cases", "journals", "logseq-docs"] {
        let graph = bitacora_testkit::graph(name);
        let b = build(Some(&graph), &[]);
        let stats = b.indexer.reconcile().unwrap();
        assert!(stats.errors.is_empty(), "{name}: {:?}", stats.errors);
        let (src, _) = source(&b, ContentPolicy::default());
        let mut ids = std::collections::BTreeSet::new();
        let mut n = 0;
        for path in src.file_paths().unwrap() {
            for d in src.file_docs(&path).unwrap() {
                assert!(d.text.starts_with("# "), "{name}/{path}");
                assert!(
                    ids.insert(d.doc_id.clone()),
                    "{name}: duplicate {}",
                    d.doc_id
                );
                n += 1;
            }
        }
        assert!(n > 0, "{name} produced no documents");
    }
}

// ------------------------------------------------------------- end to end

#[derive(Default)]
struct Mock {
    calls: Mutex<Vec<(String, String)>>,
    hits: AtomicUsize,
}

async fn handle(op: &'static str, m: Arc<Mock>, body: Value) -> axum::response::Response {
    m.hits.fetch_add(1, Ordering::SeqCst);
    let path = body["file_path"].as_str().unwrap_or_default().to_owned();
    m.calls.lock().unwrap().push((op.to_owned(), path.clone()));
    axum::Json(json!({"file_path": path, "action": "created", "status": "deleted"})).into_response()
}

async fn serve(mock: Arc<Mock>) -> String {
    let app = Router::new()
        .route(
            "/api/v1/remembrances/kb/documents",
            post(
                |State(m): State<Arc<Mock>>, axum::Json(b): axum::Json<Value>| {
                    handle("upsert", m, b)
                },
            )
            .delete(
                |State(m): State<Arc<Mock>>, axum::Json(b): axum::Json<Value>| {
                    handle("delete", m, b)
                },
            ),
        )
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn real_index_events_drive_exactly_the_needed_requests() {
    let b = build(
        None,
        &[
            ("pages/Trip Plan.md", TRIP),
            ("pages/Health.md", HEALTH),
            ("journals/2024_05_01.md", JOURNAL),
        ],
    );
    let events = b.indexer.subscribe();
    b.indexer.reconcile().unwrap();
    let (src, _) = source(&b, ContentPolicy::default());

    let mock = Arc::new(Mock::default());
    let base = serve(Arc::clone(&mock)).await;
    let client = PandoClient::new(PandoConfig::new(base)).unwrap();
    let ledger = Arc::new(Ledger::open_in_memory("r").unwrap());
    let mut worker = SemanticWorker::start(
        &tokio::runtime::Handle::current(),
        SemanticParams {
            source: Arc::new(src.clone()),
            ledger: Arc::clone(&ledger),
            policy: Arc::new(parking_lot::RwLock::new(ContentPolicy::default())),
            events,
            kb: Arc::new(move || Some(client.kb())),
            gate: Arc::new(|| true),
            sink: None,
            config: SenderConfig {
                debounce: Duration::from_millis(30),
                coalesce: Duration::from_millis(20),
                idle_poll: Duration::from_millis(40),
                ..SenderConfig::default()
            },
        },
    )
    .unwrap();

    // Eligible: the ferry block, the packing thought, the walk. Private page, property-only,
    // asset-only, short blocks and the pre-block are not sent.
    wait_for("cold start", || ledger.synced_count().unwrap() == 3).await;
    wait_for("drained", || ledger.pending_count().unwrap() == 0).await;
    let docs: Vec<String> = mock
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|(_, p)| p.clone())
        .collect();
    assert_eq!(docs.len(), 3, "{docs:?}");
    assert!(docs.iter().all(|d| !d.contains("Health")));

    // Edit exactly one block on disk; the index reports it and one upsert follows.
    let edited = TRIP.replace(
        "packing light for the trip",
        "packing light for the long trip",
    );
    write(&b.graph, "pages/Trip Plan.md", &edited);
    b.indexer.reconcile().unwrap();
    wait_for("edit upsert", || mock.hits.load(Ordering::SeqCst) == 4).await;
    wait_for("drained", || ledger.pending_count().unwrap() == 0).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(mock.hits.load(Ordering::SeqCst), 4);
    let last = mock.calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.0, "upsert");
    assert!(
        last.1.ends_with("6650a1b2-0000-4000-8000-00000000000d"),
        "{last:?}"
    );

    // Deleting a file deletes its documents.
    std::fs::remove_file(b.graph.join("journals/2024_05_01.md")).unwrap();
    b.indexer.reconcile().unwrap();
    wait_for("delete", || mock.hits.load(Ordering::SeqCst) == 5).await;
    let last = mock.calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.0, "delete");
    assert!(last.1.ends_with("6650a1b2-0000-4000-8000-00000000000e"));
    worker.stop();
    b.indexer.shutdown();
}
