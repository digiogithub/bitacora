//! Cold-build benchmark on a synthetic ~50k-block graph (BIT-T-0047, design §4.6: < 5 s).
//!
//! Ignored by default; CI runs it as a non-blocking report:
//! `cargo test -p bitacora-index --release --test bench_cold_build -- --ignored --nocapture`
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use bitacora_config::EffectiveConfig;
use bitacora_index::{Indexer, IndexerOptions};
use common::{count, env, synth};

#[test]
fn generated_graph_is_deterministic_and_indexes_completely() {
    let env = env();
    let spec = synth::Spec {
        pages: 20,
        journals: 10,
        blocks_per_page: 12,
        seed: 3,
    };
    let blocks = synth::generate(&env.graph, &spec);
    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let stats = ix.reconcile().expect("reconcile");
    assert_eq!(stats.scanned, 30);
    assert!(stats.errors.is_empty(), "{:?}", stats.errors);
    // Every generated bullet is a block; pages tagged in the header add a pre-block.
    assert!(count(&index, "SELECT count(*) FROM blocks") >= i64::try_from(blocks).unwrap());
}

/// CI smoke of the cold-build benchmark (BIT-SP-0003.R16): ~5k blocks, a relaxed bound that
/// holds in debug builds, a clean `foreign_key_check` and a no-op warm reconcile.
#[test]
fn cold_build_smoke_is_fast_and_consistent() {
    let env = env();
    synth::generate(&env.graph, &synth::Spec::small());
    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let t = std::time::Instant::now();
    let stats = ix.reconcile().expect("reconcile");
    let cold = t.elapsed();
    assert!(stats.errors.is_empty(), "{:?}", stats.errors);
    let bound = if cfg!(debug_assertions) {
        std::time::Duration::from_secs(30)
    } else {
        std::time::Duration::from_secs(2)
    };
    assert!(cold < bound, "cold build of ~5k blocks took {cold:?}");
    let fk = index
        .reader()
        .expect("reader")
        .prepare("PRAGMA foreign_key_check")
        .expect("prepare")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .count();
    assert_eq!(fk, 0, "foreign_key_check returned rows");
    assert_eq!(ix.reconcile().expect("warm").parsed, 0);
}

#[test]
#[ignore = "benchmark"]
fn cold_build_of_a_50k_block_graph() {
    let env = env();
    let t = std::time::Instant::now();
    let blocks = synth::generate(&env.graph, &synth::Spec::fifty_k());
    eprintln!("generated {blocks} blocks in {:?}", t.elapsed());

    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let t = std::time::Instant::now();
    let stats = ix.reconcile().expect("reconcile");
    let cold = t.elapsed();
    eprintln!(
        "cold build: {} files, {} blocks, {:?} ({} errors)",
        stats.parsed,
        count(&index, "SELECT count(*) FROM blocks"),
        cold,
        stats.errors.len()
    );
    let size = std::fs::metadata(index.location().db_path()).map_or(0, |m| m.len());
    eprintln!("index size: {} MiB", size / (1024 * 1024));

    let t = std::time::Instant::now();
    let again = ix.reconcile().expect("warm");
    eprintln!(
        "warm reconcile (nothing changed): {:?}, parsed {}",
        t.elapsed(),
        again.parsed
    );
    assert_eq!(again.parsed, 0);
    if cold.as_secs_f64() >= 5.0 {
        eprintln!("WARNING: cold build above the 5 s target");
    }
}

/// Writes the 5,000-page / 50k-block benchmark graph used by the app performance report to the
/// directory named by `BITACORA_BENCH_OUT` (BIT-T-0337):
/// `BITACORA_BENCH_OUT=/path cargo test -p bitacora-index --release --test bench_cold_build -- --ignored generate_app_bench_graph`
#[test]
#[ignore = "writes a large graph to BITACORA_BENCH_OUT"]
fn generate_app_bench_graph() {
    let Some(out) = std::env::var_os("BITACORA_BENCH_OUT") else {
        eprintln!("set BITACORA_BENCH_OUT to a directory");
        return;
    };
    let out = std::path::Path::new(&out);
    let spec = synth::Spec {
        pages: 5000,
        journals: 200,
        blocks_per_page: 10,
        seed: 11,
    };
    let blocks = synth::generate(out, &spec);
    synth::generate_big_page(out, "Big page", 5000, 99);
    synth::generate_recent_journals(out, 14, 25, 1234);
    // Three query widgets for the query-refresh measurements of the app benchmark.
    std::fs::write(
        out.join("pages/Query bench.md"),
        "- {{query (and [[budget]] [[design]] [[release]])}}\n- {{query (task TODO DOING)}}\n- {{query [[Page 7]]}}\n",
    )
    .expect("write query page");
    eprintln!(
        "wrote {} blocks + a 5,000-block page to {}",
        blocks,
        out.display()
    );
}
