//! Force-directed graph layout (ADR-034): a deterministic, GPUI-free simulation with
//! Logseq-equivalent parameters (link 70, charge -600 within range 600, collide 26, gravity,
//! velocity decay 0.5), Barnes-Hut many-body repulsion over a quadtree, and a background thread
//! that publishes `Arc<[[f32; 2]]>` position snapshots.
//!
//! The crate depends on no other Bitacora crate: callers convert index data into [`GraphInput`].

mod params;
mod quadtree;
mod rng;
mod sim;
mod worker;

pub use params::{ForceParams, GraphInput};
pub use sim::Simulation;
pub use worker::{Control, SimulationHandle, Snapshot};
