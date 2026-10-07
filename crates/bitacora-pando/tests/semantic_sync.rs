#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Semantic sync worker against a mock Pando KB (BIT-US-0143, BIT-SP-0010.R3).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use bitacora_index::IndexEvent;
use bitacora_pando::EventSink;
use bitacora_pando::PandoEvent;

use bitacora_pando::semantic::{
    ContentPolicy, DocSource, Gate, KbProvider, Ledger, SemanticDoc, SemanticError, SemanticParams,
    SemanticWorker, SenderConfig, SharedPolicy, doc_id,
};
use pando::{PandoClient, PandoConfig};
use serde_json::{Value, json};

const GRAPH: &str = "g0g0g0g0g0g0g0g0";

// ---------------------------------------------------------------- fake index

type Files = BTreeMap<String, Vec<(String, String)>>;

/// `path -> [(uuid, text)]`, mutated by the tests like the index would be.
#[derive(Default, Clone)]
struct FakeSource {
    files: Arc<Mutex<Files>>,
    policy: SharedPolicy,
}

impl FakeSource {
    fn set(&self, path: &str, blocks: &[(&str, &str)]) {
        let v = blocks
            .iter()
            .map(|(u, t)| ((*u).to_owned(), (*t).to_owned()))
            .collect();
        self.files.lock().unwrap().insert(path.to_owned(), v);
    }

    fn remove(&self, path: &str) {
        self.files.lock().unwrap().remove(path);
    }

    fn doc(&self, path: &str, uuid: &str, text: &str) -> Option<SemanticDoc> {
        if self.policy.read().is_excluded(path, path) {
            return None;
        }
        let mut metadata = serde_json::Map::new();
        metadata.insert("page_path".into(), json!(path));
        Some(SemanticDoc {
            doc_id: doc_id(GRAPH, uuid),
            block_uuid: uuid.to_owned(),
            file_path: path.to_owned(),
            text: text.to_owned(),
            metadata,
            tags: vec![],
            content_hash: blake3::hash(format!("{path}|{text}").as_bytes()).to_hex()[..32]
                .to_owned(),
        })
    }
}

impl DocSource for FakeSource {
    fn file_paths(&self) -> Result<Vec<String>, SemanticError> {
        Ok(self.files.lock().unwrap().keys().cloned().collect())
    }

    fn file_docs(&self, path: &str) -> Result<Vec<SemanticDoc>, SemanticError> {
        let files = self.files.lock().unwrap();
        Ok(files
            .get(path)
            .into_iter()
            .flatten()
            .filter_map(|(u, t)| self.doc(path, u, t))
            .collect())
    }

    fn block_doc(&self, uuid: &str) -> Result<Option<SemanticDoc>, SemanticError> {
        let files = self.files.lock().unwrap();
        for (p, blocks) in files.iter() {
            if let Some((u, t)) = blocks.iter().find(|(u, _)| u == uuid) {
                return Ok(self.doc(p, u, t));
            }
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------- mock Pando

#[derive(Default)]
struct Mock {
    /// ("upsert" | "delete", file_path, metadata)
    calls: Mutex<Vec<(String, String, Value)>>,
    offline: AtomicBool,
    /// doc ids answered with 400.
    reject: Mutex<Vec<String>>,
    hits: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
    delay: Mutex<Duration>,
}

impl Mock {
    fn calls_of(&self, op: &str) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(o, _, _)| o == op)
            .map(|(_, p, _)| p.clone())
            .collect()
    }

    fn total(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

async fn handle(op: &'static str, mock: Arc<Mock>, body: Value) -> Response {
    mock.hits.fetch_add(1, Ordering::SeqCst);
    let now = mock.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
    mock.max_in_flight.fetch_max(now, Ordering::SeqCst);
    let delay = *mock.delay.lock().unwrap();
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    mock.in_flight.fetch_sub(1, Ordering::SeqCst);
    if mock.offline.load(Ordering::SeqCst) {
        return (StatusCode::SERVICE_UNAVAILABLE, "down").into_response();
    }
    let path = body["file_path"].as_str().unwrap_or_default().to_owned();
    if mock.reject.lock().unwrap().contains(&path) {
        return (StatusCode::BAD_REQUEST, axum::Json(json!({"error": "bad"}))).into_response();
    }
    mock.calls.lock().unwrap().push((
        op.to_owned(),
        path.clone(),
        body.get("metadata").cloned().unwrap_or(Value::Null),
    ));
    let out = if op == "upsert" {
        json!({"file_path": path, "action": "created"})
    } else {
        json!({"file_path": path, "status": "deleted"})
    };
    (StatusCode::OK, axum::Json(out)).into_response()
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

// ---------------------------------------------------------------- harness

fn fast() -> SenderConfig {
    SenderConfig {
        debounce: Duration::from_millis(30),
        coalesce: Duration::from_millis(10),
        concurrency: 2,
        batch: 16,
        backoff_base: Duration::from_millis(40),
        backoff_max: Duration::from_millis(200),
        idle_poll: Duration::from_millis(40),
    }
}

struct Harness {
    source: FakeSource,
    base: String,
    events: Sender<IndexEvent>,
    worker: Option<SemanticWorker>,
    ledger: Arc<Ledger>,
    sink: EventSink,
}

fn replaced(path: &str) -> IndexEvent {
    IndexEvent::FileReplaced {
        file_id: 1,
        path: path.to_owned(),
        page_ids_touched: vec![],
        block_uuids_added: vec![],
        block_uuids_removed: vec![],
    }
}

impl Harness {
    async fn new(ledger: Arc<Ledger>, source: FakeSource, mock: Arc<Mock>) -> Self {
        let base = serve(Arc::clone(&mock)).await;
        let mut h = Self {
            source,
            base,
            events: channel().0,
            worker: None,
            ledger,
            sink: EventSink::default(),
        };
        h.start();
        h
    }

    /// (Re)starts the worker like a new session: same ledger file, fresh event channel.
    fn start(&mut self) {
        self.worker = None;
        let (tx, rx) = channel();
        self.events = tx;
        let client = PandoClient::new(PandoConfig::new(self.base.clone())).expect("client");
        let kb: KbProvider = Arc::new(move || Some(client.kb()));
        let gate: Gate = Arc::new(|| true);
        let params = SemanticParams {
            source: Arc::new(self.source.clone()),
            ledger: Arc::clone(&self.ledger),
            policy: Arc::clone(&self.source.policy),
            events: rx,
            kb,
            gate,
            sink: Some(self.sink.clone()),
            config: fast(),
        };
        self.worker =
            Some(SemanticWorker::start(&tokio::runtime::Handle::current(), params).expect("start"));
    }

    fn event(&self, path: &str) {
        self.events.send(replaced(path)).expect("send");
    }

    fn worker(&self) -> &SemanticWorker {
        self.worker.as_ref().expect("worker")
    }
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn settle(h: &Harness) {
    wait_for("outbox to drain", || h.ledger.pending_count().unwrap() == 0).await;
}

fn id(u: &str) -> String {
    doc_id(GRAPH, u)
}

const PAGE_A: &[(&str, &str)] = &[("a1", "first block"), ("a2", "second block")];
const PAGE_B: &[(&str, &str)] = &[("b1", "third block")];

// ---------------------------------------------------------------- scenarios

#[tokio::test(flavor = "multi_thread")]
async fn cold_start_sends_everything_then_a_restart_sends_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let db = dir.path().join("semantic.sqlite");
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    source.set("pages/B.md", PAGE_B);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open(&db, "remote").expect("ledger"));
    let mut h = Harness::new(Arc::clone(&ledger), source, Arc::clone(&mock)).await;
    wait_for("3 upserts", || mock.calls_of("upsert").len() == 3).await;
    settle(&h).await;
    assert_eq!(ledger.synced_count().unwrap(), 3);
    // Metadata travels with the document.
    assert!(
        mock.calls
            .lock()
            .unwrap()
            .iter()
            .all(|(_, _, m)| m["page_path"].is_string())
    );

    // App restart with no changes (even with a reopened ledger file): nothing is sent.
    drop(h.worker.take());
    drop(ledger);
    h.ledger = Arc::new(Ledger::open(&db, "remote").expect("reopen"));
    h.start();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(mock.total(), 3, "{:?}", mock.calls.lock().unwrap());
    assert_eq!(h.worker().status().synced, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_edit_sends_exactly_one_upsert() {
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(ledger, source.clone(), Arc::clone(&mock)).await;
    wait_for("initial sync", || mock.calls_of("upsert").len() == 2).await;
    settle(&h).await;

    source.set(
        "pages/A.md",
        &[("a1", "first block"), ("a2", "second block edited")],
    );
    h.event("pages/A.md");
    wait_for("edit upsert", || mock.calls_of("upsert").len() == 3).await;
    settle(&h).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let ups = mock.calls_of("upsert");
    assert_eq!(ups.len(), 3);
    assert_eq!(ups[2], id("a2"));
    assert!(mock.calls_of("delete").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn burst_of_edits_to_one_block_collapses_into_one_upsert() {
    let source = FakeSource::default();
    source.set("pages/A.md", &[("a1", "first block")]);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(ledger, source.clone(), Arc::clone(&mock)).await;
    wait_for("initial", || mock.total() == 1).await;
    settle(&h).await;
    for i in 0..5 {
        source.set("pages/A.md", &[("a1", &format!("first block v{i}"))]);
        h.event("pages/A.md");
    }
    wait_for("collapsed upsert", || mock.calls_of("upsert").len() >= 2).await;
    settle(&h).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(mock.calls_of("upsert").len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_edits_survive_a_restart_and_flush_when_pando_returns() {
    let dir = tempfile::tempdir().expect("dir");
    let db = dir.path().join("semantic.sqlite");
    let source = FakeSource::default();
    let blocks: Vec<(String, String)> = (0..7)
        .map(|i| (format!("u{i}"), format!("block number {i}")))
        .collect();
    let as_refs = |v: &[(String, String)]| -> Vec<(String, String)> { v.to_vec() };
    let set = |s: &FakeSource, v: &[(String, String)]| {
        let r: Vec<(&str, &str)> = v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        s.set("pages/A.md", &r);
    };
    set(&source, &as_refs(&blocks));
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open(&db, "remote").expect("ledger"));
    let mut h = Harness::new(Arc::clone(&ledger), source.clone(), Arc::clone(&mock)).await;
    wait_for("initial 7", || mock.calls_of("upsert").len() == 7).await;
    settle(&h).await;

    // Pando goes away; 5 blocks are edited and 1 deleted.
    mock.offline.store(true, Ordering::SeqCst);
    let mut edited = blocks.clone();
    for b in edited.iter_mut().take(5) {
        b.1.push_str(" (edited)");
    }
    edited.remove(6);
    set(&source, &edited);
    h.event("pages/A.md");
    wait_for("outbox filled", || ledger.pending_count().unwrap() == 6).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        ledger.pending_count().unwrap(),
        6,
        "nothing is lost while offline"
    );
    assert_eq!(mock.total(), 7);

    // The app quits and restarts while Pando is still down: the outbox is on disk.
    drop(h.worker.take());
    drop(ledger);
    h.ledger = Arc::new(Ledger::open(&db, "remote").expect("reopen"));
    assert_eq!(h.ledger.pending_count().unwrap(), 6);
    h.start();

    mock.offline.store(false, Ordering::SeqCst);
    settle(&h).await;
    assert_eq!(mock.calls_of("upsert").len(), 7 + 5);
    assert_eq!(mock.calls_of("delete"), [id("u6")]);
    assert_eq!(h.ledger.synced_count().unwrap(), 6);
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_stops_the_round_instead_of_hammering_the_server() {
    let source = FakeSource::default();
    let blocks: Vec<(String, String)> = (0..20)
        .map(|i| (format!("u{i}"), format!("block number {i}")))
        .collect();
    let r: Vec<(&str, &str)> = blocks
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    source.set("pages/A.md", &r);
    let mock = Arc::new(Mock::default());
    mock.offline.store(true, Ordering::SeqCst);
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(Arc::clone(&ledger), source, Arc::clone(&mock)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(ledger.pending_count().unwrap(), 20);
    let hits = mock.hits.load(Ordering::SeqCst);
    assert!(hits <= 10, "{hits} requests for 20 documents while offline");
    mock.offline.store(false, Ordering::SeqCst);
    h.worker().retry_now();
    settle(&h).await;
    assert_eq!(mock.calls_of("upsert").len(), 20);
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrency_is_bounded() {
    let source = FakeSource::default();
    let blocks: Vec<(String, String)> = (0..12)
        .map(|i| (format!("u{i}"), format!("block number {i}")))
        .collect();
    let r: Vec<(&str, &str)> = blocks
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    source.set("pages/A.md", &r);
    let mock = Arc::new(Mock::default());
    *mock.delay.lock().unwrap() = Duration::from_millis(40);
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(ledger, source, Arc::clone(&mock)).await;
    wait_for("12 upserts", || mock.calls_of("upsert").len() == 12).await;
    settle(&h).await;
    let max = mock.max_in_flight.load(Ordering::SeqCst);
    assert!((1..=2).contains(&max), "max in flight {max}");
    assert_eq!(max, 2, "work is actually parallel");
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_files_and_blocks_delete_their_documents() {
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    source.set("pages/B.md", PAGE_B);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(Arc::clone(&ledger), source.clone(), Arc::clone(&mock)).await;
    wait_for("initial", || mock.calls_of("upsert").len() == 3).await;
    settle(&h).await;

    source.remove("pages/B.md");
    h.events
        .send(IndexEvent::FileDeleted {
            path: "pages/B.md".into(),
            page_ids_touched: vec![],
            block_uuids_removed: vec!["b1".into()],
        })
        .expect("send");
    wait_for("delete", || mock.calls_of("delete").len() == 1).await;
    assert_eq!(mock.calls_of("delete"), [id("b1")]);
    settle(&h).await;
    assert_eq!(ledger.synced_count().unwrap(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_block_moved_between_files_is_not_deleted() {
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    source.set("pages/B.md", PAGE_B);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(ledger, source.clone(), Arc::clone(&mock)).await;
    wait_for("initial", || mock.calls_of("upsert").len() == 3).await;
    settle(&h).await;

    // a2 moves from A to B; the index reports A first, then B.
    source.set("pages/A.md", &[("a1", "first block")]);
    source.set(
        "pages/B.md",
        &[("b1", "third block"), ("a2", "second block")],
    );
    h.event("pages/A.md");
    h.event("pages/B.md");
    wait_for("moved upsert", || mock.calls_of("upsert").len() == 4).await;
    settle(&h).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(mock.calls_of("delete").is_empty(), "moved, not removed");
    let last = mock.calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.2["page_path"], json!("pages/B.md"));
}

#[tokio::test(flavor = "multi_thread")]
async fn new_exclusions_delete_documents_already_in_pando() {
    let source = FakeSource::default();
    source.set("pages/work/plan.md", &[("w1", "quarterly plan")]);
    source.set("pages/home.md", &[("h1", "buy groceries")]);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(Arc::clone(&ledger), source, Arc::clone(&mock)).await;
    wait_for("initial", || mock.calls_of("upsert").len() == 2).await;
    settle(&h).await;

    h.worker().set_policy(ContentPolicy {
        exclusions: vec!["pages/work/".into()],
        ..ContentPolicy::default()
    });
    wait_for("exclusion delete", || mock.calls_of("delete").len() == 1).await;
    assert_eq!(mock.calls_of("delete"), [id("w1")]);
    settle(&h).await;
    assert_eq!(ledger.synced_count().unwrap(), 1);

    // Lifting the exclusion sends it again.
    h.worker().set_policy(ContentPolicy::default());
    wait_for("re-upsert", || mock.calls_of("upsert").len() == 3).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn purge_deletes_everything_the_ledger_knows() {
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    source.set("pages/B.md", PAGE_B);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(Arc::clone(&ledger), source, Arc::clone(&mock)).await;
    wait_for("initial", || mock.calls_of("upsert").len() == 3).await;
    settle(&h).await;
    h.worker().purge();
    wait_for("3 deletes", || mock.calls_of("delete").len() == 3).await;
    settle(&h).await;
    assert_eq!(ledger.synced_count().unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_document_does_not_block_the_others() {
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    let mock = Arc::new(Mock::default());
    mock.reject.lock().unwrap().push(id("a1"));
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let _h = Harness::new(Arc::clone(&ledger), source, Arc::clone(&mock)).await;
    wait_for("the good one", || mock.calls_of("upsert") == [id("a2")]).await;
    wait_for("the bad one parked", || {
        ledger
            .due(i64::MAX, 10)
            .unwrap()
            .iter()
            .any(|e| e.doc_id == id("a1") && e.attempts >= 1)
    })
    .await;
    assert_eq!(ledger.pending_count().unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_events_reach_the_total() {
    let source = FakeSource::default();
    source.set("pages/A.md", PAGE_A);
    source.set("pages/B.md", PAGE_B);
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let sink = EventSink::default();
    let rx = sink.subscribe();
    let mut h = Harness::new(Arc::clone(&ledger), source, Arc::clone(&mock)).await;
    // Replace the worker so it publishes into our subscribed sink.
    h.sink = sink;
    h.start();
    wait_for("synced", || ledger.synced_count().unwrap() == 3).await;
    settle(&h).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let progress: Vec<_> = rx
        .try_iter()
        .filter_map(|e| match e {
            PandoEvent::SyncProgress(p) => Some(p),
            _ => None,
        })
        .collect();
    let last = progress.last().expect("progress events");
    assert_eq!(last.total, Some(3));
    assert_eq!(last.done, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ledger_for_another_remote_starts_empty() {
    let dir = tempfile::tempdir().expect("dir");
    let db = dir.path().join("semantic.sqlite");
    let a = Ledger::open(&db, "http://one").expect("a");
    a.enqueue_upsert("d1", "u1", "p.md", "h1").expect("enqueue");
    assert_eq!(a.pending_count().unwrap(), 1);
    drop(a);
    let same = Ledger::open(&db, "http://one").expect("same");
    assert_eq!(same.pending_count().unwrap(), 1);
    drop(same);
    let other = Ledger::open(&db, "http://two").expect("other");
    assert_eq!(other.pending_count().unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn rebuilt_index_with_new_block_uuids_replaces_documents() {
    // A rebuild gives generated uuids new values: the old documents are deleted via the ledger.
    let source = FakeSource::default();
    source.set(
        "pages/A.md",
        &[("old1", "an unmarked block"), ("old2", "another one")],
    );
    let mock = Arc::new(Mock::default());
    let ledger = Arc::new(Ledger::open_in_memory("r").expect("ledger"));
    let h = Harness::new(Arc::clone(&ledger), source.clone(), Arc::clone(&mock)).await;
    wait_for("initial", || mock.calls_of("upsert").len() == 2).await;
    settle(&h).await;
    source.set(
        "pages/A.md",
        &[("new1", "an unmarked block"), ("new2", "another one")],
    );
    h.events.send(IndexEvent::BulkFinished).expect("send");
    wait_for("swap", || {
        mock.calls_of("upsert").len() == 4 && mock.calls_of("delete").len() == 2
    })
    .await;
    settle(&h).await;
    assert_eq!(ledger.synced_count().unwrap(), 2);
    let mut deleted = mock.calls_of("delete");
    deleted.sort();
    assert_eq!(deleted, [id("old1"), id("old2")]);
}
