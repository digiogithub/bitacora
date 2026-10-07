//! Layout scaling benchmark (BIT-US-0161): construction, per-tick cost, ticks and wall time
//! until settled, and idle CPU of the worker once settled, for 3k, 5k, 10k and 20k nodes.
//! Ignored by default (release builds only):
//! `cargo test -p bitacora-graph --release --test bench_scale -- --ignored --nocapture`
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use bitacora_graph::{ForceParams, GraphInput, Simulation, SimulationHandle};

/// Sparse graph with a heavy-tailed degree distribution (preferential-ish attachment), about
/// 1.5 links per node, like a notes graph with a few hub pages.
fn notes_graph(n: usize, seed: u64) -> GraphInput {
    let mut s = seed | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut ends: Vec<u32> = vec![0];
    let mut links = Vec::new();
    for i in 1..n as u32 {
        let k = 1 + (next() % 2) as usize;
        for _ in 0..k {
            // Half uniform, half proportional to current degree (hubs).
            let t = if next() % 2 == 0 {
                (next() % u64::from(i)) as u32
            } else {
                ends[(next() % ends.len() as u64) as usize]
            };
            links.push((i, t));
            ends.push(t);
            ends.push(i);
        }
    }
    GraphInput {
        node_count: n,
        links,
    }
}

/// User + system CPU time of this process (Linux `/proc/self/stat`), zero elsewhere.
fn process_cpu() -> Duration {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return Duration::ZERO;
    };
    let rest = stat.rsplit(')').next().unwrap_or("");
    let f: Vec<&str> = rest.split_whitespace().collect();
    // After the comm field: utime is index 11, stime index 12 (fields 14 and 15 of the file).
    let ticks: u64 = [11, 12]
        .iter()
        .filter_map(|i| f.get(*i).and_then(|v| v.parse::<u64>().ok()))
        .sum();
    Duration::from_millis(ticks * 10)
}

fn pct(sorted: &[Duration], p: f64) -> Duration {
    sorted[(((sorted.len() as f64) * p).ceil() as usize).saturating_sub(1)]
}

#[test]
#[ignore = "benchmark"]
fn layout_scaling() {
    for n in [3_000usize, 5_000, 10_000, 20_000] {
        let g = notes_graph(n, 11);
        let t = Instant::now();
        let mut sim = Simulation::new(&g, ForceParams::default(), 1);
        let build = t.elapsed();
        let mut ticks = Vec::new();
        let start = Instant::now();
        loop {
            let t = Instant::now();
            if !sim.tick() {
                break;
            }
            ticks.push(t.elapsed());
        }
        let wall = start.elapsed();
        let count = ticks.len();
        ticks.sort();
        eprintln!(
            "nodes {n:>6} links {:>6}: build {build:>9.2?}  ticks {count}  tick p50 {:>9.2?} p95 {:>9.2?}  settle wall {wall:>9.2?}",
            g.links.len(),
            pct(&ticks, 0.5),
            pct(&ticks, 0.95),
        );
    }
}

#[test]
#[ignore = "benchmark"]
fn settled_worker_is_idle() {
    let g = notes_graph(5_000, 11);
    let sim = Simulation::new(&g, ForceParams::default(), 1);
    let h = SimulationHandle::spawn(sim, Duration::from_millis(16)).expect("spawn");
    let t = Instant::now();
    while !h.snapshot().settled {
        assert!(t.elapsed() < Duration::from_secs(120), "never settled");
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("5000 nodes settled after {:?}", t.elapsed());
    let (c0, w0) = (process_cpu(), Instant::now());
    std::thread::sleep(Duration::from_secs(5));
    let (cpu, wall) = (process_cpu() - c0, w0.elapsed());
    eprintln!("idle once settled: {cpu:?} CPU in {wall:?}");
    assert!(
        cpu < Duration::from_millis(100),
        "a settled layout must not burn CPU"
    );
}
