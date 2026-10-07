#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! v2 Pando-side benchmarks (BIT-US-0161): hybrid search latency and the initial semantic sync
//! backlog throughput against an in-process mock KB server. Release builds only, ignored:
//! `cargo test -p bitacora-pando --release --test bench_v2 -- --ignored --nocapture --test-threads=1`
//! `BENCH_PAGES` (default 1000, 50 blocks per page) scales the graph; 10000 gives ~540k blocks.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::routing::post;
use bitacora_config::EffectiveConfig;
use bitacora_index::{Index, IndexLocation, Indexer, IndexerOptions, OpenOptions, graph_id};
use bitacora_pando::semantic::{
    ContentPolicy, DocSource, Gate, GraphInfo, HybridOptions, HybridSearch, IndexSource,
    KbProvider, Ledger, Remote, SemanticParams, SemanticWorker, SenderConfig, SharedPolicy,
};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

const WORDS: [&str; 16] = [
    "project", "meeting", "idea", "review", "budget", "design", "release", "plan", "research",
    "draft", "notes", "customer", "roadmap", "sprint", "decision", "launch",
];

#[derive(Default)]
struct Mock {
    upserts: AtomicUsize,
    delay: Mutex<Duration>,
    hits: Mutex<Vec<Value>>,
}

async fn serve(mock: Arc<Mock>) -> String {
    let app = Router::new()
        .route(
            "/api/v1/remembrances/kb/documents",
            post(
                |State(m): State<Arc<Mock>>, axum::Json(b): axum::Json<Value>| async move {
                    let delay = *m.delay.lock().unwrap();
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    m.upserts.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({"file_path": b["file_path"], "action": "created"}))
                },
            ),
        )
        .route(
            "/api/v1/remembrances/kb/search",
            post(|State(m): State<Arc<Mock>>| async move {
                let delay = *m.delay.lock().unwrap();
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                let results = m.hits.lock().unwrap().clone();
                axum::Json(json!({ "results": results }))
            }),
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

fn generate(root: &Path, pages: usize) -> usize {
    let mut s = 17u64;
    let mut next = move || {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        s >> 33
    };
    let dir = root.join("pages");
    std::fs::create_dir_all(&dir).expect("dir");
    let mut blocks = 0;
    for p in 0..pages {
        let mut text = String::new();
        for n in 0..50 {
            let mut line = String::from("- ");
            for _ in 0..(5 + next() % 8) {
                line.push_str(WORDS[(next() % 16) as usize]);
                line.push(' ');
            }
            let _ = writeln!(text, "{line}item {p}-{n}");
            blocks += 1;
        }
        std::fs::write(dir.join(format!("Page {p}.md")), text).expect("write");
    }
    blocks
}

fn pct(sorted: &[Duration], p: f64) -> Duration {
    sorted[(((sorted.len() as f64) * p).ceil() as usize).saturating_sub(1)]
}

fn config(concurrency: usize) -> SenderConfig {
    SenderConfig {
        debounce: Duration::from_millis(10),
        coalesce: Duration::from_millis(10),
        concurrency,
        batch: 64,
        idle_poll: Duration::from_millis(20),
        ..SenderConfig::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark"]
async fn semantic_backlog_and_hybrid_search() {
    let pages: usize = std::env::var("BENCH_PAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let tmp = tempfile::tempdir().unwrap();
    let graph = tmp.path().join("graph");
    let blocks = generate(&graph, pages);
    let cfg = EffectiveConfig::default();
    let loc = IndexLocation::in_data_dir(&tmp.path().join("data"), &graph).unwrap();
    let index = Index::open(loc, OpenOptions::for_config(&graph, &cfg)).unwrap();
    let indexer = Indexer::start(&index, IndexerOptions::new(&graph, cfg)).unwrap();
    let t = Instant::now();
    indexer.reconcile().unwrap();
    eprintln!(
        "{pages} pages, {blocks} blocks, cold build {:?}",
        t.elapsed()
    );

    let reader = index.read_api();
    let id = graph_id(&graph).unwrap();
    let info = GraphInfo {
        id: id.clone(),
        name: "bench".into(),
    };
    let policy: SharedPolicy = Arc::new(parking_lot::RwLock::new(ContentPolicy::default()));
    let source = Arc::new(IndexSource::new(reader.clone(), info, Arc::clone(&policy)));

    // Reading the whole graph through the DocSource (what the reconcile pass does).
    let t = Instant::now();
    let mut docs = 0;
    let mut sample_uuids = Vec::new();
    for path in source.file_paths().unwrap() {
        let d = source.file_docs(&path).unwrap();
        docs += d.len();
        if sample_uuids.len() < 40 {
            sample_uuids.extend(d.iter().take(2).map(|x| x.doc_id.clone()));
        }
    }
    let scan = t.elapsed();
    eprintln!(
        "DocSource scan: {docs} docs in {scan:?} ({:.0} docs/s)",
        docs as f64 / scan.as_secs_f64()
    );

    let mock = Arc::new(Mock::default());
    let base = serve(Arc::clone(&mock)).await;

    // Initial backlog through the real worker.
    for (delay_ms, conc) in [(0u64, 4usize), (20, 4), (20, 16)] {
        *mock.delay.lock().unwrap() = Duration::from_millis(delay_ms);
        mock.upserts.store(0, Ordering::SeqCst);
        let client = PandoClient::new(PandoConfig::new(base.clone())).expect("client");
        let kb: KbProvider = Arc::new(move || Some(client.kb()));
        let gate: Gate = Arc::new(|| true);
        let ledger = Arc::new(Ledger::open_in_memory("bench").expect("ledger"));
        let (_tx, rx) = channel();
        let params = SemanticParams {
            source: source.clone(),
            ledger: Arc::clone(&ledger),
            policy: Arc::clone(&policy),
            events: rx,
            kb,
            gate,
            sink: None,
            config: config(conc),
        };
        let t = Instant::now();
        let worker =
            SemanticWorker::start(&tokio::runtime::Handle::current(), params).expect("start");
        let mut enqueued_at = None;
        loop {
            let synced = ledger.synced_count().unwrap();
            let pending = ledger.pending_count().unwrap();
            if enqueued_at.is_none() && pending > 0 {
                enqueued_at = Some(t.elapsed());
            }
            if synced as usize >= docs && pending == 0 {
                break;
            }
            assert!(
                t.elapsed() < Duration::from_secs(1800),
                "stuck at {synced}/{docs}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let total = t.elapsed();
        eprintln!(
            "backlog delay {delay_ms:>2} ms conc {conc:>2}: {docs} docs in {total:?} ({:.0} docs/s), first outbox rows after {:?}",
            docs as f64 / total.as_secs_f64(),
            enqueued_at.unwrap_or_default()
        );
        drop(worker);
    }

    // Hybrid search: mock KB answers with 20 real doc ids.
    *mock.hits.lock().unwrap() = sample_uuids
        .iter()
        .take(20)
        .map(|d| json!({"file_path": d, "chunk_content": "x", "score": 0.9}))
        .collect();
    let client = PandoClient::new(PandoConfig::new(base)).expect("client");
    let kb: KbProvider = Arc::new(move || Some(client.kb()));
    let gate: Gate = Arc::new(|| true);
    let remote = Remote {
        handle: tokio::runtime::Handle::current(),
        kb,
        gate,
    };
    let lexical = HybridSearch::new(reader.clone(), source.clone(), id.clone(), None);
    let hybrid = HybridSearch::new(reader, source, id, Some(remote));
    for (name, searcher, delay_ms) in [
        ("lexical only", &lexical, 0u64),
        ("hybrid, mock 0 ms", &hybrid, 0),
        ("hybrid, mock 50 ms", &hybrid, 50),
        ("hybrid, mock 300 ms", &hybrid, 300),
    ] {
        *mock.delay.lock().unwrap() = Duration::from_millis(delay_ms);
        let mut samples = Vec::new();
        for i in 0..40 {
            let q = [
                "roadmap",
                "budget meeting",
                "release plan",
                "customer draft",
            ][i % 4];
            let s = searcher.clone();
            let t = Instant::now();
            tokio::task::spawn_blocking(move || {
                s.search(q, &HybridOptions::default()).expect("search")
            })
            .await
            .unwrap();
            samples.push(t.elapsed());
        }
        samples.sort();
        eprintln!(
            "search {name:<22} p50 {:>9.2?} p95 {:>9.2?} max {:>9.2?}",
            pct(&samples, 0.5),
            pct(&samples, 0.95),
            samples.last().unwrap()
        );
    }
    indexer.shutdown();
}
