//! The force simulation (single-threaded core, deterministic for a given seed).
//!
//! Forces follow the classic velocity-Verlet-free "alpha cooling" scheme: every tick applies
//! link, many-body, collide and gravity forces to node velocities scaled by `alpha`, then
//! integrates positions. Written from the documented behaviour of such simulations, with
//! Logseq's graph-view parameters as defaults.

use std::sync::Arc;

mod collide;

use collide::Grid;

use crate::params::{ForceParams, GraphInput};
use crate::quadtree::{NONE, Quadtree};
use crate::rng::Lcg;

/// A force-directed layout of one graph.
#[derive(Debug)]
pub struct Simulation {
    pos: Vec<[f32; 2]>,
    vel: Vec<[f32; 2]>,
    pinned: Vec<Option<[f32; 2]>>,
    links: Vec<(u32, u32)>,
    link_strength: Vec<f32>,
    link_bias: Vec<f32>,
    params: ForceParams,
    alpha: f32,
    rng: Lcg,
    tree: Quadtree,
    charge_weight: Vec<f32>,
    predicted: Vec<[f32; 2]>,
    order: Vec<u32>,
    grid: Grid,
    ticks: u64,
    /// Per-tick alpha decay overriding the parameters' (quick cool-down after a drag).
    fast_decay: Option<f32>,
}

impl Simulation {
    /// Lay out `input` from a deterministic phyllotaxis start derived from `seed`.
    #[must_use]
    pub fn new(input: &GraphInput, params: ForceParams, seed: u64) -> Self {
        let n = input.node_count;
        let mut rng = Lcg::new(seed);
        let offset = rng.next_f32() * std::f32::consts::TAU;
        let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
        let pos: Vec<[f32; 2]> = (0..n)
            .map(|i| {
                let r = 10.0 * (0.5 + i as f32).sqrt();
                let a = i as f32 * golden + offset;
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        let links: Vec<(u32, u32)> = input
            .links
            .iter()
            .copied()
            .filter(|&(a, b)| a != b && (a as usize) < n && (b as usize) < n)
            .collect();
        let mut count = vec![0u32; n];
        for &(a, b) in &links {
            count[a as usize] += 1;
            count[b as usize] += 1;
        }
        let link_strength = links
            .iter()
            .map(|&(a, b)| 1.0 / count[a as usize].min(count[b as usize]).max(1) as f32)
            .collect();
        let link_bias = links
            .iter()
            .map(|&(a, b)| {
                let (ca, cb) = (count[a as usize] as f32, count[b as usize] as f32);
                ca / (ca + cb)
            })
            .collect();
        Self {
            vel: vec![[0.0; 2]; n],
            pinned: vec![None; n],
            predicted: vec![[0.0; 2]; n],
            order: Vec::new(),
            grid: Grid::default(),
            charge_weight: vec![params.charge; n],
            pos,
            links,
            link_strength,
            link_bias,
            params,
            alpha: 1.0,
            rng,
            tree: Quadtree::default(),
            ticks: 0,
            fast_decay: None,
        }
    }

    /// Like [`Simulation::new`], but nodes with `Some(position)` in `initial` start there (a
    /// refresh that keeps surviving nodes in place, BIT-SP-0012.R6) and the layout starts at
    /// `alpha` instead of fully hot. Missing entries keep the phyllotaxis start.
    #[must_use]
    pub fn with_positions(
        input: &GraphInput,
        params: ForceParams,
        seed: u64,
        initial: &[Option<[f32; 2]>],
        alpha: f32,
    ) -> Self {
        let mut sim = Self::new(input, params, seed);
        for (slot, start) in sim.pos.iter_mut().zip(initial) {
            if let Some(at) = start {
                *slot = *at;
            }
        }
        sim.alpha = alpha.clamp(0.0, 1.0);
        sim
    }

    /// Lays `input` out completely off-screen and returns the frozen result: nodes with
    /// `Some` in `initial` start there, those flagged in `pinned` are held exactly in place
    /// during the layout (a refresh keeps surviving nodes put), and the others find their
    /// equilibrium around them. Runs at most `max_ticks` ticks, then freezes.
    #[must_use]
    pub fn layout_offscreen(
        input: &GraphInput,
        params: ForceParams,
        seed: u64,
        initial: &[Option<[f32; 2]>],
        pinned: &[bool],
        max_ticks: u32,
    ) -> Self {
        let any_pinned = pinned.iter().any(|p| *p);
        let alpha = if any_pinned { 0.5 } else { 1.0 };
        let mut sim = Self::with_positions(input, params, seed, initial, alpha);
        for (i, held) in pinned.iter().enumerate() {
            if *held && let Some(Some(at)) = initial.get(i) {
                sim.pin(i, *at);
            }
        }
        sim.settle(max_ticks);
        for i in 0..sim.len() {
            sim.unpin(i);
        }
        sim
    }

    /// Ticks until cooled (at most `max_ticks`), then freezes. Returns the ticks run.
    pub fn settle(&mut self, max_ticks: u32) -> u32 {
        let mut n = 0;
        while n < max_ticks && self.tick() {
            n += 1;
        }
        self.freeze();
        n
    }

    /// Stops all motion for good: velocities are zeroed and alpha drops to 0, so the layout
    /// is settled until something reheats it.
    pub fn freeze(&mut self) {
        self.vel.fill([0.0; 2]);
        self.alpha = 0.0;
        self.fast_decay = None;
    }

    /// Make the layout cool down within about `ticks` ticks from its current alpha (the end
    /// of a drag). A later [`Simulation::reheat`] restores the normal cooling.
    pub fn cool_quickly(&mut self, ticks: u32) {
        let min = self.params.alpha_min.clamp(1e-6, 0.999);
        let from = self.alpha.max(min * 2.0);
        let decay = 1.0 - (min / from).powf(1.0 / ticks.max(1) as f32);
        self.fast_decay = Some(decay.clamp(0.0, 1.0));
        self.params.alpha_target = 0.0;
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pos.len()
    }

    /// No nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Current positions.
    #[must_use]
    pub fn positions(&self) -> &[[f32; 2]] {
        &self.pos
    }

    /// Copy of the positions for sharing with another thread.
    #[must_use]
    pub fn snapshot(&self) -> Arc<[[f32; 2]]> {
        Arc::from(self.pos.as_slice())
    }

    /// Current alpha (1 = hot, below `alpha_min` = settled).
    #[must_use]
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Number of ticks run so far.
    #[must_use]
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Cooled down: further ticks would not move anything noticeably.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.alpha < self.params.alpha_min && self.params.alpha_target < self.params.alpha_min
    }

    /// Sum of squared velocities (a proxy for the layout energy).
    #[must_use]
    pub fn kinetic_energy(&self) -> f32 {
        self.vel.iter().map(|v| v[0] * v[0] + v[1] * v[1]).sum()
    }

    /// Current parameters.
    #[must_use]
    pub fn params(&self) -> ForceParams {
        self.params
    }

    /// Replace the parameters (takes effect on the next tick; does not reheat).
    pub fn set_params(&mut self, params: ForceParams) {
        self.params = params;
        self.charge_weight.fill(params.charge);
    }

    /// Raise alpha to at least `alpha` so the layout moves again.
    pub fn reheat(&mut self, alpha: f32) {
        self.fast_decay = None;
        self.alpha = self.alpha.max(alpha.clamp(0.0, 1.0));
    }

    /// Hold `node` at `at` (dragging); it still repels and collides.
    pub fn pin(&mut self, node: usize, at: [f32; 2]) {
        if let Some(slot) = self.pinned.get_mut(node) {
            *slot = Some(at);
        }
    }

    /// Release a pinned node.
    pub fn unpin(&mut self, node: usize) {
        if let Some(slot) = self.pinned.get_mut(node) {
            *slot = None;
        }
    }

    /// Advance one tick. Returns `false` once the simulation has settled (nothing was done).
    pub fn tick(&mut self) -> bool {
        if self.is_settled() {
            return false;
        }
        let p = self.params;
        let decay = self.fast_decay.unwrap_or_else(|| p.alpha_decay());
        self.alpha += (p.alpha_target - self.alpha) * decay;
        let alpha = self.alpha;
        self.apply_links(alpha);
        self.apply_many_body(alpha);
        for _ in 0..p.collide_iterations {
            self.apply_collide();
        }
        self.apply_gravity(alpha);
        let keep = 1.0 - p.velocity_decay.clamp(0.0, 1.0);
        for i in 0..self.pos.len() {
            if let Some(at) = self.pinned[i] {
                self.pos[i] = at;
                self.vel[i] = [0.0; 2];
                continue;
            }
            for a in 0..2 {
                let v = self.vel[i][a] * keep;
                // Non-finite values (degenerate parameters) are reset instead of propagating.
                self.vel[i][a] = if v.is_finite() { v } else { 0.0 };
                let x = self.pos[i][a] + self.vel[i][a];
                self.pos[i][a] = if x.is_finite() { x } else { 0.0 };
            }
        }
        self.ticks += 1;
        if self.is_settled() {
            // Residual velocity (collide is not scaled by alpha) must not survive the cool-down:
            // a settled layout is exactly frozen.
            self.freeze();
        }
        true
    }

    fn apply_links(&mut self, alpha: f32) {
        for (k, &(s, t)) in self.links.iter().enumerate() {
            let (s, t) = (s as usize, t as usize);
            let mut dx = self.pos[t][0] + self.vel[t][0] - self.pos[s][0] - self.vel[s][0];
            let mut dy = self.pos[t][1] + self.vel[t][1] - self.pos[s][1] - self.vel[s][1];
            if dx == 0.0 {
                dx = self.rng.jiggle();
            }
            if dy == 0.0 {
                dy = self.rng.jiggle();
            }
            let len = (dx * dx + dy * dy).sqrt();
            let f = (len - self.params.link_distance) / len * alpha * self.link_strength[k];
            let (fx, fy) = (dx * f, dy * f);
            let b = self.link_bias[k];
            self.vel[t][0] -= fx * b;
            self.vel[t][1] -= fy * b;
            self.vel[s][0] += fx * (1.0 - b);
            self.vel[s][1] += fy * (1.0 - b);
        }
    }

    fn apply_gravity(&mut self, alpha: f32) {
        let g = self.params.gravity * alpha;
        for (p, v) in self.pos.iter().zip(self.vel.iter_mut()) {
            v[0] -= p[0] * g;
            v[1] -= p[1] * g;
        }
    }

    /// Barnes-Hut repulsion: distant cells act as one point mass when `size / distance < theta`.
    fn apply_many_body(&mut self, alpha: f32) {
        if self.pos.len() < 2 || self.params.charge == 0.0 {
            return;
        }
        let theta2 = (self.params.theta * self.params.theta).max(1e-6);
        let dmin2 = 1.0f32;
        let dmax2 = self.params.charge_range * self.params.charge_range;
        self.tree.build(&self.pos);
        self.tree.aggregate(&self.pos, &self.charge_weight);
        let mut stack: Vec<(u32, f32, f32, f32)> = Vec::with_capacity(64);
        self.tree.leaf_order(&mut self.order);
        for oi in 0..self.order.len() {
            let i = self.order[oi] as usize;
            let [nx, ny] = self.pos[i];
            let (mut ax, mut ay) = (0.0f32, 0.0f32);
            stack.clear();
            stack.push((0, self.tree.x0, self.tree.y0, self.tree.size));
            while let Some((ci, x0, y0, size)) = stack.pop() {
                let cell = &self.tree.cells[ci as usize];
                if cell.value == 0.0 && !cell.internal && cell.first == NONE {
                    continue;
                }
                let mut dx = cell.cx - nx;
                let mut dy = cell.cy - ny;
                let mut l = dx * dx + dy * dy;
                if size * size / theta2 < l {
                    if l < dmax2 && cell.value != 0.0 {
                        if dx == 0.0 {
                            dx = self.rng.jiggle();
                            l += dx * dx;
                        }
                        if dy == 0.0 {
                            dy = self.rng.jiggle();
                            l += dy * dy;
                        }
                        if l < dmin2 {
                            l = (dmin2 * l).sqrt();
                        }
                        ax += dx * cell.value * alpha / l;
                        ay += dy * cell.value * alpha / l;
                    }
                    continue;
                }
                if l >= dmax2 {
                    continue;
                }
                if cell.internal {
                    for (q, &c) in cell.child.iter().enumerate() {
                        if c != NONE {
                            let (cx0, cy0, cs) = Quadtree::child_bounds(x0, y0, size, q);
                            stack.push((c, cx0, cy0, cs));
                        }
                    }
                } else {
                    let mut q = cell.first;
                    while q != NONE {
                        if q as usize != i {
                            let mut dx = self.pos[q as usize][0] - nx;
                            let mut dy = self.pos[q as usize][1] - ny;
                            let mut l = dx * dx + dy * dy;
                            if dx == 0.0 {
                                dx = self.rng.jiggle();
                                l += dx * dx;
                            }
                            if dy == 0.0 {
                                dy = self.rng.jiggle();
                                l += dy * dy;
                            }
                            if l < dmin2 {
                                l = (dmin2 * l).sqrt();
                            }
                            let w = self.charge_weight[q as usize] * alpha / l;
                            ax += dx * w;
                            ay += dy * w;
                        }
                        q = self.tree.next[q as usize];
                    }
                }
            }
            self.vel[i][0] += ax;
            self.vel[i][1] += ay;
        }
    }
}
