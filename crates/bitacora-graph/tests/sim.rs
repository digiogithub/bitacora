//! Simulation behaviour: determinism, finiteness, cooling, forces, pinning, worker thread, speed.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use bitacora_graph::{Control, ForceParams, GraphInput, Simulation, SimulationHandle};
use proptest::prelude::*;

fn ring(n: usize) -> GraphInput {
    GraphInput {
        node_count: n,
        links: (0..n as u32).map(|i| (i, (i + 1) % n as u32)).collect(),
    }
}

/// Pseudo-random sparse graph (xorshift), `n` nodes, ~2 links per node.
fn random_graph(n: usize, seed: u64) -> GraphInput {
    let mut s = seed | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let links = (0..n * 2)
        .map(|_| ((next() % n as u64) as u32, (next() % n as u64) as u32))
        .collect();
    GraphInput {
        node_count: n,
        links,
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[test]
fn same_seed_same_layout_different_seed_differs() {
    let g = random_graph(200, 7);
    let run = |seed| {
        let mut s = Simulation::new(&g, ForceParams::default(), seed);
        for _ in 0..100 {
            s.tick();
        }
        s.positions().to_vec()
    };
    assert_eq!(run(1), run(1));
    assert_ne!(run(1), run(2));
}

#[test]
fn empty_and_tiny_graphs_do_not_panic() {
    for n in 0..4 {
        let mut s = Simulation::new(&ring(n.max(1)).clone(), ForceParams::default(), 1);
        let _ = n;
        while s.tick() {}
        assert!(s.positions().iter().flatten().all(|v| v.is_finite()));
    }
    let mut s = Simulation::new(&GraphInput::default(), ForceParams::default(), 1);
    assert!(s.is_empty());
    s.tick();
}

#[test]
fn cools_down_and_settles() {
    let mut s = Simulation::new(&random_graph(100, 3), ForceParams::default(), 5);
    let mut last = s.alpha();
    let mut ticks = 0;
    while s.tick() {
        assert!(s.alpha() <= last, "alpha never increases");
        last = s.alpha();
        ticks += 1;
        assert!(ticks < 1000);
    }
    assert!(s.is_settled());
    assert!(
        (290..=310).contains(&ticks),
        "d3-like 300 tick cooling: {ticks}"
    );
}

#[test]
fn energy_decreases_as_the_layout_cools() {
    let mut s = Simulation::new(&random_graph(300, 11), ForceParams::default(), 9);
    let mut early = 0.0f32;
    for i in 0..300 {
        if !s.tick() {
            break;
        }
        if i == 30 {
            early = s.kinetic_energy();
        }
    }
    assert!(early > 0.0);
    assert!(
        s.kinetic_energy() < early * 0.1,
        "{} vs {early}",
        s.kinetic_energy()
    );
}

#[test]
fn links_pull_and_charge_pushes() {
    let g = GraphInput {
        node_count: 2,
        links: vec![(0, 1)],
    };
    let mut s = Simulation::new(&g, ForceParams::default(), 1);
    while s.tick() {}
    let d = dist(s.positions()[0], s.positions()[1]);
    // Spring rests at 70, repulsion and collide (52) push slightly beyond.
    assert!((52.0..140.0).contains(&d), "{d}");

    let unlinked = GraphInput {
        node_count: 2,
        links: vec![],
    };
    let mut s = Simulation::new(&unlinked, ForceParams::default(), 1);
    let start = dist(s.positions()[0], s.positions()[1]);
    while s.tick() {}
    assert!(dist(s.positions()[0], s.positions()[1]) > start);
}

#[test]
fn collide_keeps_nodes_apart() {
    let g = GraphInput {
        node_count: 40,
        links: vec![],
    };
    let params = ForceParams {
        charge: 0.0,
        gravity: 0.1,
        ..ForceParams::default()
    };
    let mut s = Simulation::new(&g, params, 4);
    for _ in 0..300 {
        s.tick();
    }
    let p = s.positions();
    let mut min = f32::MAX;
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            min = min.min(dist(p[i], p[j]));
        }
    }
    assert!(min > 40.0, "min distance {min} (collision diameter 52)");
}

#[test]
fn many_body_matches_brute_force_closely() {
    // theta 0 degenerates to exact pairwise; the default must stay near it.
    let g = random_graph(150, 21);
    let exact = ForceParams {
        theta: 0.0,
        collide_iterations: 0,
        ..ForceParams::default()
    };
    let approx = ForceParams {
        collide_iterations: 0,
        ..ForceParams::default()
    };
    let (mut a, mut b) = (
        Simulation::new(&g, exact, 3),
        Simulation::new(&g, approx, 3),
    );
    a.tick();
    b.tick();
    let mut worst = 0.0f32;
    for (p, q) in a.positions().iter().zip(b.positions()) {
        worst = worst.max(dist(*p, *q));
    }
    assert!(worst < 5.0, "first-tick divergence {worst}");
}

#[test]
fn pinned_nodes_stay_put_and_unpin_releases() {
    let mut s = Simulation::new(&ring(10), ForceParams::default(), 1);
    s.pin(3, [500.0, -500.0]);
    for _ in 0..50 {
        s.tick();
    }
    assert_eq!(s.positions()[3], [500.0, -500.0]);
    s.unpin(3);
    s.reheat(0.5);
    for _ in 0..50 {
        s.tick();
    }
    assert_ne!(s.positions()[3], [500.0, -500.0]);
}

#[test]
fn invalid_links_are_ignored() {
    let g = GraphInput {
        node_count: 3,
        links: vec![(0, 0), (0, 9), (1, 2)],
    };
    let mut s = Simulation::new(&g, ForceParams::default(), 1);
    while s.tick() {}
    assert!(s.positions().iter().flatten().all(|v| v.is_finite()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn positions_stay_finite_and_alpha_cools(
        n in 0usize..120,
        seed in any::<u64>(),
        gseed in any::<u64>(),
        charge in -2000.0f32..0.0,
        distance in 1.0f32..300.0,
    ) {
        let g = if n == 0 { GraphInput::default() } else { random_graph(n, gseed) };
        let params = ForceParams { charge, link_distance: distance, ..ForceParams::default() };
        let mut s = Simulation::new(&g, params, seed);
        let mut last = s.alpha();
        for _ in 0..350 {
            if !s.tick() { break; }
            prop_assert!(s.alpha() <= last);
            last = s.alpha();
            prop_assert!(s.positions().iter().flatten().all(|v| v.is_finite()));
        }
        prop_assert!(s.kinetic_energy().is_finite());
    }
}

#[test]
fn worker_publishes_snapshots_and_settles() {
    let sim = Simulation::new(&random_graph(50, 2), ForceParams::default(), 1);
    let h = SimulationHandle::spawn(sim, Duration::ZERO).expect("spawn");
    let t = Instant::now();
    loop {
        let s = h.snapshot();
        if s.settled {
            assert_eq!(s.positions.len(), 50);
            assert!(s.generation > 100);
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(20), "did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn worker_pause_reheat_and_drag() {
    let sim = Simulation::new(&ring(20), ForceParams::default(), 1);
    let h = SimulationHandle::spawn(sim, Duration::from_millis(1)).expect("spawn");
    h.send(Control::Pause(true));
    std::thread::sleep(Duration::from_millis(60));
    let a = h.snapshot().generation;
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(h.snapshot().generation, a, "paused");
    h.send(Control::Pause(false));
    h.send(Control::Pin {
        node: 0,
        at: [300.0, 300.0],
    });
    let t = Instant::now();
    while h.snapshot().generation <= a + 5 {
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(h.snapshot().positions[0], [300.0, 300.0]);
    h.send(Control::Unpin(0));
    h.send(Control::SetParams {
        params: ForceParams::default(),
        reheat: 0.5,
    });
    drop(h); // joins the thread
}

/// Performance gate (release only): 5k nodes must sustain at least 60 ticks per second.
#[test]
fn five_thousand_nodes_at_sixty_ticks_per_second() {
    if cfg!(debug_assertions) {
        eprintln!("skipped in debug builds; run `cargo test -p bitacora-graph --release`");
        return;
    }
    let mut s = Simulation::new(&random_graph(5000, 42), ForceParams::default(), 1);
    for _ in 0..5 {
        s.tick();
    }
    // Best of five 20-tick windows, so a busy CI host does not turn this into a flake.
    let mut best = 0.0f64;
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..20 {
            s.tick();
        }
        best = best.max(20.0 / t.elapsed().as_secs_f64());
    }
    eprintln!("5k nodes: {best:.0} ticks/s");
    assert!(best >= 60.0, "{best:.1} ticks/s");
}
