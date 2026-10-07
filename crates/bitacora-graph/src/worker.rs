//! Background simulation thread: control messages in, position snapshots out.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::params::ForceParams;
use crate::sim::Simulation;

/// Messages the UI sends to the simulation thread.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    /// Replace the force parameters and reheat to `reheat` alpha.
    SetParams {
        /// New parameters.
        params: ForceParams,
        /// Alpha to reheat to (0 = do not reheat).
        reheat: f32,
    },
    /// Pause or resume ticking.
    Pause(bool),
    /// Raise alpha to at least this value.
    Reheat(f32),
    /// Hold a node at a position (dragging); also keeps the layout warm while held.
    Pin {
        /// Node index.
        node: usize,
        /// Position in layout space.
        at: [f32; 2],
    },
    /// Release a pinned node and let the layout cool again.
    Unpin(usize),
}

/// A published layout state.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Increments on every published tick.
    pub generation: u64,
    /// Alpha at that tick.
    pub alpha: f32,
    /// Settled: no further snapshots until a control message reheats.
    pub settled: bool,
    /// Node positions.
    pub positions: Arc<[[f32; 2]]>,
}

enum Msg {
    Control(Control),
    Stop,
}

/// Owner of the simulation thread; dropping it stops and joins the thread.
#[derive(Debug)]
pub struct SimulationHandle {
    tx: Sender<Msg>,
    latest: Arc<Mutex<Snapshot>>,
    join: Option<JoinHandle<()>>,
}

fn lock(m: &Mutex<Snapshot>) -> MutexGuard<'_, Snapshot> {
    // A poisoned lock only means a panic elsewhere; the snapshot itself is always valid.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl SimulationHandle {
    /// Run `sim` on a new thread, ticking at most every `tick_interval` (`Duration::ZERO` =
    /// as fast as possible). Returns `None` if the thread cannot be spawned.
    #[must_use]
    pub fn spawn(sim: Simulation, tick_interval: Duration) -> Option<Self> {
        let latest = Arc::new(Mutex::new(Snapshot {
            generation: 0,
            alpha: sim.alpha(),
            settled: sim.is_settled(),
            positions: sim.snapshot(),
        }));
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&latest);
        let join = std::thread::Builder::new()
            .name("bitacora-graph-sim".into())
            .spawn(move || run(sim, &rx, &shared, tick_interval))
            .ok()?;
        Some(Self {
            tx,
            latest,
            join: Some(join),
        })
    }

    /// Send a control message. Ignored if the thread has stopped.
    pub fn send(&self, control: Control) {
        let _ = self.tx.send(Msg::Control(control));
    }

    /// The most recent snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.latest).clone()
    }
}

impl Drop for SimulationHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Ticks a released drag takes to cool down (about 0.7 s at 60 Hz), then everything is frozen.
const RELEASE_COOL_TICKS: u32 = 40;
/// Alpha kept while a node is held, so its neighbours follow.
const DRAG_ALPHA: f32 = 0.3;

/// Apply one message. Returns `false` on stop.
fn handle(sim: &mut Simulation, paused: &mut bool, held: &mut HashSet<usize>, msg: Msg) -> bool {
    let Msg::Control(c) = msg else {
        return false;
    };
    match c {
        Control::SetParams { params, reheat } => {
            sim.set_params(params);
            sim.reheat(reheat);
        }
        Control::Pause(p) => *paused = p,
        Control::Reheat(a) => sim.reheat(a),
        Control::Pin { node, at } => {
            // One `Pin` arrives per pointer move: track the held *set*, not message counts.
            if held.is_empty() {
                let mut p = sim.params();
                p.alpha_target = DRAG_ALPHA;
                sim.set_params(p);
            }
            held.insert(node);
            sim.reheat(DRAG_ALPHA);
            sim.pin(node, at);
        }
        Control::Unpin(node) => {
            sim.unpin(node);
            if held.remove(&node) && held.is_empty() {
                let mut p = sim.params();
                p.alpha_target = 0.0;
                sim.set_params(p);
                sim.cool_quickly(RELEASE_COOL_TICKS);
            }
        }
    }
    true
}

fn run(mut sim: Simulation, rx: &Receiver<Msg>, shared: &Mutex<Snapshot>, tick_interval: Duration) {
    let (mut paused, mut held) = (false, HashSet::new());
    let mut generation = 0u64;
    loop {
        // Idle (paused or settled): block until something arrives.
        while paused || sim.is_settled() {
            match rx.recv() {
                Ok(m) => {
                    if !handle(&mut sim, &mut paused, &mut held, m) {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
        let started = Instant::now();
        sim.tick();
        generation += 1;
        *lock(shared) = Snapshot {
            generation,
            alpha: sim.alpha(),
            settled: sim.is_settled(),
            positions: sim.snapshot(),
        };
        // Sleep out the rest of the frame, but wake for messages.
        let mut remaining = tick_interval.saturating_sub(started.elapsed());
        loop {
            let msg = if remaining.is_zero() {
                match rx.try_recv() {
                    Ok(m) => m,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            } else {
                let t = Instant::now();
                match rx.recv_timeout(remaining) {
                    Ok(m) => {
                        remaining = remaining.saturating_sub(t.elapsed());
                        m
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            if !handle(&mut sim, &mut paused, &mut held, msg) {
                return;
            }
        }
    }
}
