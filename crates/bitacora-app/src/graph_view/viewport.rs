//! Pan / zoom transform between layout space and the canvas (pure, unit-tested).

use super::model::GraphModel;

/// Smallest and largest zoom factors.
pub const MIN_ZOOM: f32 = 0.05;
/// See [`MIN_ZOOM`].
pub const MAX_ZOOM: f32 = 8.0;
/// Labels are drawn above this zoom.
pub const LABEL_ZOOM: f32 = 0.9;

/// Maps layout coordinates to canvas pixels relative to the canvas centre:
/// `screen = world * zoom + offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Pixel offset of the layout origin from the canvas centre.
    pub offset: [f32; 2],
    /// Pixels per layout unit.
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// Layout point to canvas-centre-relative pixels.
    #[must_use]
    pub fn to_screen(&self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] * self.zoom + self.offset[0],
            p[1] * self.zoom + self.offset[1],
        ]
    }

    /// Canvas-centre-relative pixels to layout point.
    #[must_use]
    pub fn to_world(&self, s: [f32; 2]) -> [f32; 2] {
        [
            (s[0] - self.offset[0]) / self.zoom,
            (s[1] - self.offset[1]) / self.zoom,
        ]
    }

    /// Pans by a pixel delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset[0] += dx;
        self.offset[1] += dy;
    }

    /// Multiplies the zoom by `factor` keeping the layout point under `cursor` fixed.
    pub fn zoom_at(&mut self, cursor: [f32; 2], factor: f32) {
        let world = self.to_world(cursor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = [
            cursor[0] - world[0] * self.zoom,
            cursor[1] - world[1] * self.zoom,
        ];
    }

    /// A viewport showing all `positions` in a canvas of `size` pixels with a margin.
    #[must_use]
    pub fn fit(positions: &[[f32; 2]], size: [f32; 2]) -> Self {
        if positions.is_empty() || size[0] <= 1.0 || size[1] <= 1.0 {
            return Self::default();
        }
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in positions {
            for k in 0..2 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let margin = 48.0;
        let span = [(max[0] - min[0]).max(1.0), (max[1] - min[1]).max(1.0)];
        let zoom = ((size[0] - 2.0 * margin).max(1.0) / span[0])
            .min((size[1] - 2.0 * margin).max(1.0) / span[1])
            .clamp(MIN_ZOOM, 1.5);
        let centre = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
        Self {
            offset: [-centre[0] * zoom, -centre[1] * zoom],
            zoom,
        }
    }

    /// Whether a screen-space disc (centre relative to the canvas centre) touches a canvas of
    /// `size` pixels (culling).
    #[must_use]
    pub fn disc_visible(s: [f32; 2], radius: f32, size: [f32; 2]) -> bool {
        s[0] + radius >= -size[0] / 2.0
            && s[0] - radius <= size[0] / 2.0
            && s[1] + radius >= -size[1] / 2.0
            && s[1] - radius <= size[1] / 2.0
    }

    /// Whether the segment's bounding box touches a canvas of `size` pixels (culling).
    #[must_use]
    pub fn segment_visible(a: [f32; 2], b: [f32; 2], size: [f32; 2]) -> bool {
        let (hx, hy) = (size[0] / 2.0, size[1] / 2.0);
        !(a[0].max(b[0]) < -hx
            || a[0].min(b[0]) > hx
            || a[1].max(b[1]) < -hy
            || a[1].min(b[1]) > hy)
    }
}

/// The node under a canvas-centre-relative pixel position (the nearest one, small nodes get a
/// minimum hit radius), if any.
#[must_use]
pub fn hit_test(
    model: &GraphModel,
    positions: &[[f32; 2]],
    viewport: &Viewport,
    cursor: [f32; 2],
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, node) in model.nodes().iter().enumerate() {
        let Some(p) = positions.get(i) else { break };
        let s = viewport.to_screen(*p);
        let r = (node.radius() * viewport.zoom).max(5.0);
        let d2 = (s[0] - cursor[0]).powi(2) + (s[1] - cursor[1]).powi(2);
        if d2 <= r * r && best.is_none_or(|(_, b)| d2 <= b) {
            best = Some((i, d2));
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use bitacora_index::{GraphData, GraphDataNode};

    use super::*;

    #[test]
    fn world_screen_round_trip() {
        let v = Viewport {
            offset: [10.0, -4.0],
            zoom: 2.5,
        };
        let w = [3.0, 7.0];
        let back = v.to_world(v.to_screen(w));
        assert!((back[0] - w[0]).abs() < 1e-5 && (back[1] - w[1]).abs() < 1e-5);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut v = Viewport::default();
        v.pan(30.0, 12.0);
        let cursor = [55.0, -20.0];
        let before = v.to_world(cursor);
        v.zoom_at(cursor, 1.7);
        let after = v.to_world(cursor);
        assert!((before[0] - after[0]).abs() < 1e-4 && (before[1] - after[1]).abs() < 1e-4);
        v.zoom_at(cursor, 1e6);
        assert!((v.zoom - MAX_ZOOM).abs() < 1e-6);
        v.zoom_at(cursor, 1e-9);
        assert!((v.zoom - MIN_ZOOM).abs() < 1e-6);
    }

    #[test]
    fn fit_centres_and_scales_the_layout() {
        let pts = [[-500.0, -100.0], [500.0, 100.0]];
        let v = Viewport::fit(&pts, [400.0, 300.0]);
        assert!(v.zoom < 0.4);
        for p in pts {
            let s = v.to_screen(p);
            assert!(s[0].abs() <= 200.0 && s[1].abs() <= 150.0, "{s:?}");
        }
        assert_eq!(Viewport::fit(&[], [400.0, 300.0]), Viewport::default());
    }

    #[test]
    fn culling_rejects_far_away_shapes() {
        let size = [200.0, 100.0];
        assert!(Viewport::disc_visible([0.0, 0.0], 5.0, size));
        assert!(Viewport::disc_visible([103.0, 0.0], 5.0, size));
        assert!(!Viewport::disc_visible([300.0, 0.0], 5.0, size));
        assert!(Viewport::segment_visible([-300.0, 0.0], [300.0, 0.0], size));
        assert!(!Viewport::segment_visible(
            [150.0, 0.0],
            [300.0, 10.0],
            size
        ));
    }

    #[test]
    fn hit_test_picks_the_nearest_node() {
        let data = GraphData {
            nodes: [1, 2]
                .iter()
                .map(|&id| GraphDataNode {
                    id,
                    name: String::new(),
                    is_journal: false,
                    is_tag: false,
                    is_namespace_parent: false,
                    degree: 0,
                })
                .collect(),
            edges: Vec::new(),
        };
        let m = GraphModel::from_data(&data);
        let pos = [[0.0, 0.0], [20.0, 0.0]];
        let v = Viewport::default();
        assert_eq!(hit_test(&m, &pos, &v, [1.0, 1.0]), Some(0));
        assert_eq!(hit_test(&m, &pos, &v, [19.0, 0.0]), Some(1));
        assert_eq!(hit_test(&m, &pos, &v, [100.0, 100.0]), None);
    }
}
