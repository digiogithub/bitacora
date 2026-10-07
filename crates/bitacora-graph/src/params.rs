//! Simulation parameters and input graph.

/// Graph to lay out: `node_count` nodes (indices `0..node_count`) and undirected links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphInput {
    /// Number of nodes.
    pub node_count: usize,
    /// Links as node index pairs. Out-of-range endpoints and self-links are ignored.
    pub links: Vec<(u32, u32)>,
}

/// Force parameters. [`Default`] reproduces Logseq 0.10.x graph-view behaviour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForceParams {
    /// Rest length of link springs.
    pub link_distance: f32,
    /// Many-body strength; negative repels.
    pub charge: f32,
    /// Many-body distance max: nodes farther apart do not interact.
    pub charge_range: f32,
    /// Barnes-Hut opening angle.
    pub theta: f32,
    /// Collision radius.
    pub collide_radius: f32,
    /// Collision relaxation passes per tick.
    pub collide_iterations: u32,
    /// Pull towards the origin on each axis.
    pub gravity: f32,
    /// Fraction of velocity lost per tick.
    pub velocity_decay: f32,
    /// Alpha below which the simulation is settled.
    pub alpha_min: f32,
    /// Ticks for alpha to cool from 1 to `alpha_min` (d3 default 300).
    pub cooling_ticks: f32,
    /// Alpha the simulation cools towards (raised while dragging).
    pub alpha_target: f32,
}

impl Default for ForceParams {
    fn default() -> Self {
        Self {
            link_distance: 70.0,
            charge: -600.0,
            charge_range: 600.0,
            theta: 0.5,
            collide_radius: 26.0,
            collide_iterations: 2,
            gravity: 0.02,
            velocity_decay: 0.5,
            alpha_min: 0.001,
            cooling_ticks: 300.0,
            alpha_target: 0.0,
        }
    }
}

impl ForceParams {
    /// Per-tick alpha decay factor derived from `alpha_min` and `cooling_ticks`.
    pub(crate) fn alpha_decay(&self) -> f32 {
        let min = self.alpha_min.clamp(1e-6, 0.999);
        1.0 - min.powf(1.0 / self.cooling_ticks.max(1.0))
    }
}
