//! Arena quadtree over a point set, used by the many-body (Barnes-Hut) and collide forces.
//!
//! Points sharing a position (or too deep to separate) are chained in one leaf, so insertion
//! always terminates.

pub(crate) const NONE: u32 = u32::MAX;
const MAX_DEPTH: u32 = 40;

#[derive(Debug, Clone)]
pub(crate) struct Cell {
    /// Child cell indices (`NONE` = empty), quadrant = `2 * (y >= mid) + (x >= mid)`.
    pub(crate) child: [u32; 4],
    /// First point of the leaf chain (`NONE` for internal or empty cells).
    pub(crate) first: u32,
    pub(crate) internal: bool,
    /// Weighted centre of mass and total weight, filled by [`Quadtree::aggregate`].
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) value: f32,
}

impl Cell {
    fn leaf(first: u32) -> Self {
        Self {
            child: [NONE; 4],
            first,
            internal: false,
            cx: 0.0,
            cy: 0.0,
            value: 0.0,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Quadtree {
    pub(crate) cells: Vec<Cell>,
    /// Next point in the same leaf chain, per point.
    pub(crate) next: Vec<u32>,
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) size: f32,
}

fn quadrant(px: f32, py: f32, x0: f32, y0: f32, half: f32) -> (usize, f32, f32) {
    let qx = usize::from(px >= x0 + half);
    let qy = usize::from(py >= y0 + half);
    (qy * 2 + qx, x0 + qx as f32 * half, y0 + qy as f32 * half)
}

impl Quadtree {
    /// Rebuild the tree over `pos`, reusing allocations.
    pub(crate) fn build(&mut self, pos: &[[f32; 2]]) {
        self.cells.clear();
        self.next.clear();
        self.next.resize(pos.len(), NONE);
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pos {
            minx = minx.min(p[0]);
            miny = miny.min(p[1]);
            maxx = maxx.max(p[0]);
            maxy = maxy.max(p[1]);
        }
        if pos.is_empty() || !(minx.is_finite() && maxx.is_finite()) {
            self.x0 = 0.0;
            self.y0 = 0.0;
            self.size = 1.0;
            self.cells.push(Cell::leaf(NONE));
            return;
        }
        self.x0 = minx;
        self.y0 = miny;
        self.size = (maxx - minx).max(maxy - miny).max(1.0);
        self.cells.push(Cell::leaf(NONE));
        for i in 0..pos.len() {
            self.insert(pos, i as u32);
        }
    }

    fn insert(&mut self, pos: &[[f32; 2]], p: u32) {
        let [px, py] = pos[p as usize];
        let (mut cell, mut x0, mut y0, mut size) = (0usize, self.x0, self.y0, self.size);
        let mut depth = 0u32;
        loop {
            if self.cells[cell].internal {
                let half = size / 2.0;
                let (q, nx, ny) = quadrant(px, py, x0, y0, half);
                let c = self.cells[cell].child[q];
                if c == NONE {
                    let id = self.cells.len() as u32;
                    self.cells.push(Cell::leaf(p));
                    self.cells[cell].child[q] = id;
                    return;
                }
                cell = c as usize;
                (x0, y0, size) = (nx, ny, half);
                depth += 1;
                continue;
            }
            let head = self.cells[cell].first;
            if head == NONE {
                self.cells[cell].first = p;
                return;
            }
            let [hx, hy] = pos[head as usize];
            if (hx == px && hy == py) || depth >= MAX_DEPTH {
                self.next[p as usize] = head;
                self.cells[cell].first = p;
                return;
            }
            // Split: the existing chain moves into one child, then `p` is retried here.
            let half = size / 2.0;
            let (q, _, _) = quadrant(hx, hy, x0, y0, half);
            let id = self.cells.len() as u32;
            self.cells.push(Cell::leaf(head));
            let c = &mut self.cells[cell];
            c.first = NONE;
            c.internal = true;
            c.child[q] = id;
        }
    }

    /// Fill centre of mass and weight of every cell (`weight[i]` per point).
    pub(crate) fn aggregate(&mut self, pos: &[[f32; 2]], weight: &[f32]) {
        for ci in (0..self.cells.len()).rev() {
            let (mut sx, mut sy, mut v) = (0.0f32, 0.0f32, 0.0f32);
            if self.cells[ci].internal {
                for q in 0..4 {
                    let c = self.cells[ci].child[q];
                    if c != NONE {
                        let ch = &self.cells[c as usize];
                        sx += ch.cx * ch.value;
                        sy += ch.cy * ch.value;
                        v += ch.value;
                    }
                }
                let cell = &mut self.cells[ci];
                cell.value = v;
                if v != 0.0 {
                    cell.cx = sx / v;
                    cell.cy = sy / v;
                }
            } else {
                let mut p = self.cells[ci].first;
                let first = p;
                while p != NONE {
                    v += weight[p as usize];
                    p = self.next[p as usize];
                }
                let cell = &mut self.cells[ci];
                cell.value = v;
                if first != NONE {
                    // Chained points share one position.
                    cell.cx = pos[first as usize][0];
                    cell.cy = pos[first as usize][1];
                }
            }
        }
    }

    /// Points in depth-first leaf order (spatially coherent), for cache-friendly traversal.
    pub(crate) fn leaf_order(&self, out: &mut Vec<u32>) {
        out.clear();
        let mut stack = vec![0u32];
        while let Some(ci) = stack.pop() {
            let cell = &self.cells[ci as usize];
            if cell.internal {
                for &c in cell.child.iter().rev() {
                    if c != NONE {
                        stack.push(c);
                    }
                }
            } else {
                let mut p = cell.first;
                while p != NONE {
                    out.push(p);
                    p = self.next[p as usize];
                }
            }
        }
    }

    /// Bounds `(x0, y0, size)` of child quadrant `q` of a cell with the given bounds.
    pub(crate) fn child_bounds(x0: f32, y0: f32, size: f32, q: usize) -> (f32, f32, f32) {
        let half = size / 2.0;
        (
            x0 + (q & 1) as f32 * half,
            y0 + (q >> 1) as f32 * half,
            half,
        )
    }
}
