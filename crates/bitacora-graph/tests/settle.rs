//! Static layout behaviour (BIT-US-0157 jitter fix): off-screen settling, frozen state,
//! drag-only motion.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use bitacora_graph::{Control, ForceParams, GraphInput, Simulation, SimulationHandle};

fn ring(n: usize) -> GraphInput {
    GraphInput {
        node_count: n,
        links: (0..n as u32).map(|i| (i, (i + 1) % n as u32)).collect(),
    }
}

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

/// Regression: the UI sends one `Pin` per pointer move but one `Unpin` on release. The worker
/// used to count messages, so `alpha_target` stayed at 0.3 forever and the graph never stopped
/// moving after a drag.
#[test]
fn drag_with_many_pin_messages_cools_down_after_one_unpin() {
    let sim = Simulation::new(&ring(20), ForceParams::default(), 1);
    let h = SimulationHandle::spawn(sim, Duration::ZERO).expect("spawn");
    for i in 0..30 {
        h.send(Control::Pin {
            node: 3,
            at: [i as f32 * 4.0, 50.0],
        });
    }
    std::thread::sleep(Duration::from_millis(50));
    assert!(!h.snapshot().settled, "alive while held");
    h.send(Control::Unpin(3));
    let t = Instant::now();
    while !h.snapshot().settled {
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "never settled after the drag"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let a = h.snapshot();
    std::thread::sleep(Duration::from_millis(80));
    let b = h.snapshot();
    assert_eq!(a.generation, b.generation, "no snapshots once settled");
    assert_eq!(a.positions, b.positions);
}

#[test]
fn settle_converges_and_freezes_exactly() {
    let mut s = Simulation::new(&random_graph(60, 5), ForceParams::default(), 3);
    let ticks = s.settle(1000);
    assert!(s.is_settled() && ticks < 1000, "{ticks}");
    assert_eq!(s.kinetic_energy(), 0.0);
    let before = s.positions().to_vec();
    for _ in 0..50 {
        assert!(!s.tick());
    }
    assert_eq!(before, s.positions());
}

#[test]
fn settled_worker_publishes_nothing_and_positions_never_change() {
    let mut sim = Simulation::new(&random_graph(40, 9), ForceParams::default(), 2);
    sim.settle(1000);
    let expected = sim.positions().to_vec();
    let h = SimulationHandle::spawn(sim, Duration::ZERO).expect("spawn");
    let first = h.snapshot();
    assert!(first.settled);
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(5));
        let s = h.snapshot();
        assert_eq!(s.generation, first.generation);
        assert_eq!(&*s.positions, expected.as_slice());
    }
}

#[test]
fn offscreen_layout_keeps_pinned_nodes_exactly_in_place() {
    let input = ring(12);
    let mut initial = vec![None; 12];
    let mut pinned = vec![false; 12];
    for i in 0..10 {
        initial[i] = Some([i as f32 * 90.0, 0.0]);
        pinned[i] = true;
    }
    initial[10] = Some([5.0, 5.0]);
    let s = Simulation::layout_offscreen(&input, ForceParams::default(), 1, &initial, &pinned, 800);
    assert!(s.is_settled());
    for i in 0..10 {
        assert_eq!(s.positions()[i], [i as f32 * 90.0, 0.0]);
    }
    assert_ne!(s.positions()[10], [5.0, 5.0], "free node was laid out");
}

#[test]
fn drag_moves_neighbours_then_freezes() {
    let mut sim = Simulation::new(&ring(10), ForceParams::default(), 1);
    sim.settle(1000);
    let rest = sim.positions().to_vec();
    let h = SimulationHandle::spawn(sim, Duration::from_millis(1)).expect("spawn");
    h.send(Control::Pin {
        node: 0,
        at: [rest[0][0] + 200.0, rest[0][1]],
    });
    std::thread::sleep(Duration::from_millis(100));
    let live = h.snapshot();
    assert!(!live.settled);
    assert_ne!(live.positions[1], rest[1], "neighbour follows the drag");
    h.send(Control::Unpin(0));
    let t = Instant::now();
    while !h.snapshot().settled {
        assert!(t.elapsed() < Duration::from_secs(10), "did not stop");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn quick_cooling_after_a_reheat_stops_in_few_ticks() {
    let mut s = Simulation::new(&random_graph(30, 2), ForceParams::default(), 1);
    s.settle(1000);
    s.reheat(0.3);
    s.cool_quickly(40);
    let mut n = 0;
    while s.tick() {
        n += 1;
        assert!(n <= 200, "still running");
    }
    assert!(n <= 80, "{n}");
    assert_eq!(s.kinetic_energy(), 0.0);
}
