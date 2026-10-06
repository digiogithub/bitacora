//! Shared helpers for the runtime integration tests.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use bitacora_core::editor::BlockId;
use bitacora_core::graph::PageKey;
use bitacora_runtime::{RuntimeConfig, RuntimeEvent, Session};
use bitacora_watch::WatchConfig;

pub const PAGE: &str = "- Alpha\n- Beta\n- Gamma\n";

/// Runtime config with fast watcher timings and an index outside the graph.
pub fn config(graph: &Path, data: &Path) -> RuntimeConfig {
    let mut c = RuntimeConfig::new(graph);
    c.data_dir = Some(data.to_path_buf());
    c.global_config = None;
    c.watch = Some(WatchConfig {
        debounce: Duration::from_millis(50),
        poll_interval: Duration::from_millis(300),
        ..WatchConfig::default()
    });
    c
}

pub fn graph_with(dir: &Path, files: &[(&str, &str)]) -> PathBuf {
    let graph = dir.join("graph");
    for (rel, content) in files {
        let p = graph.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    graph
}

/// Polls `f` until it returns `Some` or the timeout expires.
pub fn wait_for<T>(what: &str, timeout: Duration, mut f: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Blocks of a loaded page as `(id, text)` in document order.
pub fn blocks(s: &Session, key: &PageKey) -> Vec<(BlockId, String)> {
    s.queue()
        .snapshot(key)
        .unwrap()
        .blocks
        .iter()
        .map(|b| (b.id, b.text.clone()))
        .collect()
}

/// Whether the index has a block whose content contains `needle`.
pub fn index_has(s: &Session, needle: &str) -> bool {
    let conn = s.index_reader().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM blocks WHERE content LIKE ?1",
            [format!("%{needle}%")],
            |r| r.get(0),
        )
        .unwrap();
    n > 0
}

/// Drains the events received so far.
pub fn drain(rx: &Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    rx.try_iter().collect()
}
