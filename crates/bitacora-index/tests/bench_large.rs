//! Large-graph benchmark (BIT-T-0336): the ~540k-block `large` preset, release builds only.
//!
//! Measures cold build, search p95, page open (first outline chunk), linked references of the
//! most referenced page and the DSL queries `(and [[a]] [[b]] [[c]])`, `(task TODO DOING)`,
//! `(between -30d today)`; the numbers decide whether `block_path_refs` is materialized
//! (design `sqlite-index-schema` section 3.1). Ignored by default:
//! `cargo test -p bitacora-index --release --test bench_large -- --ignored --nocapture`
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::time::{Duration, Instant};

use bitacora_config::EffectiveConfig;
use bitacora_core::date::Date;
use bitacora_index::query::QueryContext;
use bitacora_index::search::SearchOptions;
use bitacora_index::{Indexer, IndexerOptions};
use common::{count, env, synth};

const SEARCH_QUERIES: &[&str] = &[
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

fn report(name: &str, mut samples: Vec<Duration>, target: Duration) {
    samples.sort();
    let (p50, p95, max) = (
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        *samples.last().expect("samples"),
    );
    let verdict = if p95 < target { "ok" } else { "ABOVE TARGET" };
    eprintln!(
        "{name:<34} n={:<4} p50 {p50:>10.2?} p95 {p95:>10.2?} max {max:>10.2?}  (target < {target:?}) {verdict}",
        samples.len()
    );
}

fn time_n(n: usize, mut f: impl FnMut()) -> Vec<Duration> {
    (0..n)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect()
}

#[test]
#[ignore = "benchmark (builds a 540k-block graph)"]
fn large_graph_index_and_query_benchmarks() {
    let env = env();
    let t = Instant::now();
    let blocks = synth::generate(&env.graph, &synth::Spec::large());
    eprintln!("generated {blocks} blocks in {:?}", t.elapsed());

    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let t = Instant::now();
    let stats = ix.reconcile().expect("reconcile");
    let cold = t.elapsed();
    eprintln!(
        "cold build: {} files, {} blocks in {cold:?} ({} errors) -- target < 60 s {}",
        stats.parsed,
        count(&index, "SELECT count(*) FROM blocks"),
        stats.errors.len(),
        if cold < Duration::from_secs(60) {
            "ok"
        } else {
            "ABOVE TARGET"
        }
    );
    let size = std::fs::metadata(index.location().db_path()).map_or(0, |m| m.len());
    eprintln!("index size: {} MiB", size / (1024 * 1024));
    ix.shutdown();

    let reader = index.read_api();

    // Search.
    let opts = SearchOptions::default();
    for q in SEARCH_QUERIES {
        reader.search(q, &opts).expect("warm-up");
    }
    let mut samples = Vec::new();
    let mut per_query = vec![Duration::ZERO; SEARCH_QUERIES.len()];
    for _ in 0..10 {
        for (i, q) in SEARCH_QUERIES.iter().enumerate() {
            let t = Instant::now();
            std::hint::black_box(reader.search(q, &opts).expect("search"));
            let d = t.elapsed();
            per_query[i] = per_query[i].max(d);
            samples.push(d);
        }
    }
    for (q, d) in SEARCH_QUERIES.iter().zip(&per_query) {
        eprintln!("  max {d:>10.2?}  {q}");
    }
    report("search (14 queries)", samples, Duration::from_millis(100));

    // Page open: the first outline chunk of 40 pages.
    let ids: Vec<i64> = (0..40)
        .map(|i| {
            reader
                .page_by_name(&format!("Page {}", i * 190))
                .expect("lookup")
                .expect("page")
                .id
        })
        .collect();
    let mut it = ids.iter().cycle();
    let samples = time_n(80, || {
        let id = *it.next().expect("id");
        std::hint::black_box(reader.outline(id, 0, 100, false).expect("outline"));
    });
    report(
        "page open (outline, 100 blocks)",
        samples,
        Duration::from_millis(100),
    );

    // Linked references of the most referenced page.
    let top = count(
        &index,
        "SELECT page_id FROM block_page_refs GROUP BY page_id ORDER BY count(*) DESC LIMIT 1",
    );
    let groups = reader.linked_references(top).expect("linked refs");
    eprintln!(
        "linked refs target: {} groups, {} blocks",
        groups.len(),
        groups.iter().map(|g| g.blocks.len()).sum::<usize>()
    );
    let samples = time_n(5, || {
        std::hint::black_box(reader.linked_references(top).expect("linked refs"));
    });
    report(
        "linked refs (tag page, 41k refs)",
        samples,
        Duration::from_millis(100),
    );

    // A typical page (a few dozen references) for comparison with the extreme tag page.
    let typical = reader
        .page_by_name("Page 17")
        .expect("lookup")
        .expect("page")
        .id;
    let samples = time_n(20, || {
        std::hint::black_box(reader.linked_references(typical).expect("linked refs"));
    });
    report(
        "linked refs (typical page)",
        samples,
        Duration::from_millis(100),
    );

    // DSL queries.
    let mut ctx = QueryContext::new(Date::new(2005, 6, 1).expect("date"), 1_117_600_000_000);
    // The app shows at most 501 rows (`RESULT_CAP + 1`).
    ctx.limit = Some(501);
    for (name, q, target) in [
        (
            "query (and [[a]] [[b]] [[c]])",
            "(and [[budget]] [[design]] [[release]])",
            200,
        ),
        (
            "query (and 3 page refs)",
            "(and [[Page 1]] [[Page 2]] [[Page 3]])",
            200,
        ),
        ("query (task TODO DOING)", "(task TODO DOING)", 200),
        ("query (between -30d today)", "(between -30d today)", 200),
    ] {
        let samples = time_n(5, || {
            std::hint::black_box(reader.query_simple(q, &ctx).expect("query"));
        });
        report(name, samples, Duration::from_millis(target));
    }
}
