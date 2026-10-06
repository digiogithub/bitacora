//! Search latency on a synthetic 5,000-page graph (BIT-T-0073, p95 < 50 ms).
//!
//! Ignored by default (it builds the graph):
//! `cargo test -p bitacora-index --release --test bench_search -- --ignored --nocapture`
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::time::{Duration, Instant};

use bitacora_config::EffectiveConfig;
use bitacora_index::search::{SearchOptions, search};
use bitacora_index::{Indexer, IndexerOptions};
use common::{count, env, synth};

const QUERIES: &[&str] = &[
    "roadmap",
    "budget meeting",
    "release plan",
    "customer and risk",
    "decision not bug",
    "sprint backl",
    "\"design review\"",
    "pag 42",
    "pg4217",
    "oadma",
    "ab",
    "zzzz-no-such-term",
    "TODO launch",
    "garden health travel",
];

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let idx = ((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1);
    sorted[idx.min(sorted.len() - 1)]
}

#[test]
#[ignore = "benchmark"]
fn search_latency_on_a_5000_page_graph() {
    let env = env();
    let spec = synth::Spec {
        pages: 5000,
        journals: 200,
        blocks_per_page: 10,
        seed: 11,
    };
    let blocks = synth::generate(&env.graph, &spec);
    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let t = Instant::now();
    ix.reconcile().expect("reconcile");
    eprintln!(
        "built {} pages / {} blocks (generated {blocks}) in {:?}",
        count(
            &index,
            "SELECT count(*) FROM pages WHERE file_id IS NOT NULL"
        ),
        count(&index, "SELECT count(*) FROM blocks"),
        t.elapsed()
    );

    let reader = index.reader().expect("reader");
    let opts = SearchOptions::default();
    // Warm-up.
    for q in QUERIES {
        search(&reader, q, &opts).expect("search");
    }
    let mut samples = Vec::new();
    let mut per_query = vec![Duration::ZERO; QUERIES.len()];
    for _ in 0..20 {
        for (i, q) in QUERIES.iter().enumerate() {
            let t = Instant::now();
            let hits = search(&reader, q, &opts).expect("search");
            let d = t.elapsed();
            samples.push(d);
            per_query[i] = per_query[i].max(d);
            std::hint::black_box(hits);
        }
    }
    for (q, d) in QUERIES.iter().zip(&per_query) {
        eprintln!("  max {d:>10.2?}  {q}");
    }
    samples.sort();
    let (p50, p95, max) = (
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        *samples.last().expect("samples"),
    );
    eprintln!(
        "search latency over {} runs: p50 {p50:?}, p95 {p95:?}, max {max:?}",
        samples.len()
    );
    assert!(p95 < Duration::from_millis(50), "p95 {p95:?} >= 50 ms");
}
