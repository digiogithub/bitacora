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
