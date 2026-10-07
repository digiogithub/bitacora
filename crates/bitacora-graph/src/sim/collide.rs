//! Collision force: a uniform grid whose cells are at least one collision diameter wide, so
//! every neighbour within reach lies in the 3x3 block around a node's cell.

use super::Simulation;

/// Reusable buffers of the collision grid (counting-sort layout).
#[derive(Debug, Default)]
pub(super) struct Grid {
    start: Vec<u32>,
    fill: Vec<u32>,
    items: Vec<u32>,
}

impl Simulation {
    /// One relaxation pass over predicted positions (`pos + vel`).
    pub(super) fn apply_collide(&mut self) {
        let r = self.params.collide_radius;
        let n = self.pos.len();
        if n < 2 || r <= 0.0 {
            return;
        }
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..n {
            let p = [
                self.pos[i][0] + self.vel[i][0],
                self.pos[i][1] + self.vel[i][1],
            ];
            self.predicted[i] = p;
            minx = minx.min(p[0]);
            miny = miny.min(p[1]);
            maxx = maxx.max(p[0]);
            maxy = maxy.max(p[1]);
        }
        let reach = 2.0 * r;
        let r2 = reach * reach;
        // Grow the cell when the layout is very spread out so the grid stays O(n) in memory.
        let (w, h) = ((maxx - minx).max(1.0), (maxy - miny).max(1.0));
        let mut cell = reach;
        while (w / cell + 1.0) * (h / cell + 1.0) > (4 * n + 16) as f32 {
            cell *= 2.0;
        }
        let cols = (w / cell) as usize + 1;
        let rows = (h / cell) as usize + 1;
        let cell_of = |p: [f32; 2]| -> (usize, usize) {
            (
                (((p[0] - minx) / cell) as usize).min(cols - 1),
                (((p[1] - miny) / cell) as usize).min(rows - 1),
            )
        };
        let grid = &mut self.grid;
        grid.start.clear();
        grid.start.resize(cols * rows + 1, 0);
        for i in 0..n {
            let (cx, cy) = cell_of(self.predicted[i]);
            grid.start[cy * cols + cx + 1] += 1;
        }
        for c in 0..cols * rows {
            grid.start[c + 1] += grid.start[c];
        }
        grid.items.clear();
        grid.items.resize(n, 0);
        grid.fill.clear();
        grid.fill.extend_from_slice(&grid.start[..cols * rows]);
        for i in 0..n {
            let (cx, cy) = cell_of(self.predicted[i]);
            let slot = &mut grid.fill[cy * cols + cx];
            grid.items[*slot as usize] = i as u32;
            *slot += 1;
        }
        for i in 0..n {
            let [xi, yi] = self.predicted[i];
            let (cx, cy) = cell_of([xi, yi]);
            for gy in cy.saturating_sub(1)..=(cy + 1).min(rows - 1) {
                for gx in cx.saturating_sub(1)..=(cx + 1).min(cols - 1) {
                    let c = gy * cols + gx;
                    for k in grid.start[c]..grid.start[c + 1] {
                        let qi = grid.items[k as usize] as usize;
                        if qi <= i {
                            continue;
                        }
                        let mut dx = xi - (self.pos[qi][0] + self.vel[qi][0]);
                        let mut dy = yi - (self.pos[qi][1] + self.vel[qi][1]);
                        let mut l = dx * dx + dy * dy;
                        if l >= r2 {
                            continue;
                        }
                        if dx == 0.0 {
                            dx = self.rng.jiggle();
                            l += dx * dx;
                        }
                        if dy == 0.0 {
                            dy = self.rng.jiggle();
                            l += dy * dy;
                        }
                        let l = l.sqrt();
                        let push = (reach - l) / l * 0.5;
                        let (fx, fy) = (dx * push, dy * push);
                        // Equal radii: each node takes half of the correction.
                        self.vel[i][0] += fx;
                        self.vel[i][1] += fy;
                        self.vel[qi][0] -= fx;
                        self.vel[qi][1] -= fy;
                    }
                }
            }
        }
    }
}
