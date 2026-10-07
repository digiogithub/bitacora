//! v2 read-path benchmarks (BIT-US-0161): graph-view data load and the Tasks view queries on
//! the `large` (~540k blocks) and `fifty_k` presets, release builds only. Ignored by default:
//! `cargo test -p bitacora-index --release --test bench_v2 -- --ignored --nocapture --test-threads=1`
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use bitacora_config::EffectiveConfig;
use bitacora_index::read::TaskFilter;
use bitacora_index::{GraphFilter, Index, Indexer, IndexerOptions};
use common::{env, synth, write};

fn pct(sorted: &[Duration], p: f64) -> Duration {
    let idx = ((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1);
    sorted[idx.min(sorted.len() - 1)]
}

fn report(name: &str, target: Duration, n: usize, mut f: impl FnMut()) {
    f(); // warm-up
    let mut s: Vec<Duration> = (0..n)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect();
    s.sort();
    let p95 = pct(&s, 0.95);
    eprintln!(
        "{name:<44} p50 {:>10.2?} p95 {p95:>10.2?} max {:>10.2?} (target < {target:?}) {}",
        pct(&s, 0.5),
        s.last().expect("samples"),
        if p95 < target { "ok" } else { "ABOVE TARGET" }
    );
}

/// `pages` extra pages of ten dated open tasks each: overdue, this week and later buckets.
fn dated_tasks(root: &std::path::Path, pages: usize) {
    for p in 0..pages {
        let mut text = String::new();
        for t in 0..10 {
            let day = 1 + (p * 10 + t) % 28;
            let month = 5 + (p + t) % 3; // around the 2005-06-01 "today" of the other benches
            let kind = if t % 2 == 0 { "SCHEDULED" } else { "DEADLINE" };
            let _ = writeln!(
                text,
                "- TODO dated task {p}-{t} [[Page {p}]]\n  {kind}: <2005-{month:02}-{day:02} Mon>"
            );
        }
        write(root, &format!("pages/Dated {p}.md"), &text);
    }
}

fn build(spec: &synth::Spec, dated_pages: usize) -> (common::Env, Index) {
    let env = env();
    let t = Instant::now();
    let blocks = synth::generate(&env.graph, spec);
    dated_tasks(&env.graph, dated_pages);
    eprintln!("generated {blocks} blocks in {:?}", t.elapsed());
    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let t = Instant::now();
    ix.reconcile().expect("reconcile");
    eprintln!("cold build {:?}", t.elapsed());
    ix.shutdown();
    (env, index)
}

fn scenario(label: &str, spec: &synth::Spec, dated_pages: usize) {
    eprintln!("== {label}");
    let (_env, index) = build(spec, dated_pages);
    let reader = index.read_api();

    let default = GraphFilter::default();
    let all = GraphFilter {
        journals: true,
        ..GraphFilter::default()
    };
    let data = reader.graph_data(&default).expect("graph");
    eprintln!(
        "graph (journals off): {} nodes, {} edges",
        data.nodes.len(),
        data.edges.len()
    );
    let data_all = reader.graph_data(&all).expect("graph");
    eprintln!(
        "graph (journals on): {} nodes, {} edges",
        data_all.nodes.len(),
        data_all.edges.len()
    );
    let ms = Duration::from_millis;
    report("graph_data journals off", ms(500), 10, || {
        std::hint::black_box(reader.graph_data(&default).expect("graph"));
    });
    report("graph_data journals on", ms(500), 10, || {
        std::hint::black_box(reader.graph_data(&all).expect("graph"));
    });
    let page = data.nodes[data.nodes.len() / 2].id;
    report("local_graph_data (1 hop)", ms(100), 20, || {
        std::hint::black_box(reader.local_graph_data(page, &default).expect("local"));
    });

    let today = 20_050_601;
    let open = reader
        .task_groups(today, &TaskFilter::default())
        .expect("groups");
    eprintln!(
        "open tasks: overdue {} this week {} later {} no date {}",
        open.overdue.len(),
        open.this_week.len(),
        open.later.len(),
        open.no_date.len()
    );
    report("task_groups (all open)", ms(50), 10, || {
        std::hint::black_box(reader.task_groups(today, &TaskFilter::default()).unwrap());
    });
    let doing = TaskFilter {
        markers: vec!["DOING".into(), "NOW".into()],
        ..TaskFilter::default()
    };
    report("task_groups (DOING/NOW)", ms(50), 20, || {
        std::hint::black_box(reader.task_groups(today, &doing).unwrap());
    });
    let one_page = TaskFilter {
        page_id: Some(page),
        ..TaskFilter::default()
    };
    report("task_groups (one page)", ms(50), 20, || {
        std::hint::black_box(reader.task_groups(today, &one_page).unwrap());
    });
    report("overdue_count (sidebar badge)", ms(50), 20, || {
        std::hint::black_box(reader.overdue_count(today).unwrap());
    });
    report(
        "journal_days_with_notes (calendar month)",
        ms(50),
        20,
        || {
            std::hint::black_box(
                reader
                    .journal_days_with_notes(20_050_501, 20_050_531)
                    .unwrap(),
            );
        },
    );
}

#[test]
#[ignore = "benchmark (builds a 540k-block graph)"]
fn large_preset() {
    scenario("large preset (~540k blocks)", &synth::Spec::large(), 200);
}

#[test]
#[ignore = "benchmark"]
fn fifty_k_preset() {
    scenario("fifty_k preset (~50k blocks)", &synth::Spec::fifty_k(), 100);
}
