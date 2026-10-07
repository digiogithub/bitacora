//! Graph picture export (BIT-US-0160): SVG and PNG of the current layout.
//!
//! GPUI-free. A [`Scene`] is the node / edge / position data of the view after its filters,
//! with resolved colours; [`Scene::to_svg`] writes the SVG by hand (labels included) and
//! [`Scene::to_png`] rasterises the same scene with `tiny-skia` (circles and edges only:
//! `tiny-skia` has no text shaping, see `docs/design/graph-view.md`).

use std::fmt::Write as _;

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use super::GraphModel;

/// Empty border around the drawing, in layout units.
const MARGIN: f32 = 24.0;
/// Extra room on the right for the label of the right-most node, in layout units.
const LABEL_ROOM: f32 = 140.0;
/// Edge stroke width, in layout units.
const EDGE_WIDTH: f32 = 1.0;
/// Label font size, in layout units.
const FONT_SIZE: f32 = 11.0;
/// Largest side of a PNG, in pixels (the scale shrinks to fit).
const MAX_PNG_SIDE: f32 = 8192.0;

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    fn color(self) -> Color {
        Color::from_rgba8(self.0, self.1, self.2, 255)
    }
}

/// Colours of a scene (resolved from the theme by the view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Background fill.
    pub background: Rgb,
    /// Edge stroke.
    pub edge: Rgb,
    /// Ordinary page.
    pub node: Rgb,
    /// Journal page.
    pub journal: Rgb,
    /// Tag / namespace parent.
    pub tag: Rgb,
    /// The current page of a local graph.
    pub accent: Rgb,
    /// Label text.
    pub text: Rgb,
}

/// One drawn page.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    /// Layout position.
    pub pos: [f32; 2],
    /// Circle radius.
    pub radius: f32,
    /// Page name.
    pub label: String,
    /// Fill colour.
    pub fill: Rgb,
}

/// What an export draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// Pages.
    pub nodes: Vec<SceneNode>,
    /// Edges as node indices.
    pub edges: Vec<(usize, usize)>,
    /// Colours.
    pub palette: Palette,
    /// Whether labels are written (SVG only).
    pub labels: bool,
}

/// Why an export failed.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The graph has no pages.
    #[error("there is nothing to export")]
    Empty,
    /// The raster could not be created or encoded.
    #[error("cannot render the PNG: {0}")]
    Png(String),
}

impl Scene {
    /// Builds the scene of `model` laid out at `positions`; `current` is the highlighted node.
    pub fn from_model(
        model: &GraphModel,
        positions: &[[f32; 2]],
        current: Option<usize>,
        palette: Palette,
        labels: bool,
    ) -> Self {
        let nodes: Vec<SceneNode> = model
            .nodes()
            .iter()
            .zip(positions)
            .enumerate()
            .map(|(i, (node, pos))| SceneNode {
                pos: *pos,
                radius: node.radius(),
                label: node.name.clone(),
                fill: if Some(i) == current {
                    palette.accent
                } else if node.is_tag {
                    palette.tag
                } else if node.is_journal {
                    palette.journal
                } else {
                    palette.node
                },
            })
            .collect();
        let edges = model
            .links()
            .iter()
            .map(|&(a, b)| (a as usize, b as usize))
            .filter(|&(a, b)| a < nodes.len() && b < nodes.len())
            .collect();
        Self {
            nodes,
            edges,
            palette,
            labels,
        }
    }

    /// `(min_x, min_y, width, height)` of the drawing including the margins.
    fn frame(&self) -> Option<(f32, f32, f32, f32)> {
        let first = self.nodes.first()?;
        let mut min = [first.pos[0] - first.radius, first.pos[1] - first.radius];
        let mut max = [first.pos[0] + first.radius, first.pos[1] + first.radius];
        for n in &self.nodes {
            min[0] = min[0].min(n.pos[0] - n.radius);
            min[1] = min[1].min(n.pos[1] - n.radius);
            max[0] = max[0].max(n.pos[0] + n.radius);
            max[1] = max[1].max(n.pos[1] + n.radius);
        }
        let room = if self.labels { LABEL_ROOM } else { 0.0 };
        Some((
            min[0] - MARGIN,
            min[1] - MARGIN,
            (max[0] - min[0]) + 2.0 * MARGIN + room,
            (max[1] - min[1]) + 2.0 * MARGIN,
        ))
    }

    /// The scene as an SVG document.
    ///
    /// # Errors
    /// [`ExportError::Empty`] without nodes.
    pub fn to_svg(&self) -> Result<String, ExportError> {
        let (x, y, w, h) = self.frame().ok_or(ExportError::Empty)?;
        let p = &self.palette;
        let mut s = String::with_capacity(256 + self.nodes.len() * 120 + self.edges.len() * 50);
        let _ = writeln!(
            s,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"{x:.2} {y:.2} {w:.2} {h:.2}\">"
        );
        let _ = writeln!(
            s,
            "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" fill=\"{}\"/>",
            p.background.hex()
        );
        let _ = writeln!(
            s,
            "<g stroke=\"{}\" stroke-width=\"{EDGE_WIDTH}\" fill=\"none\">",
            p.edge.hex()
        );
        for &(a, b) in &self.edges {
            let (na, nb) = (&self.nodes[a], &self.nodes[b]);
            let _ = writeln!(
                s,
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>",
                na.pos[0], na.pos[1], nb.pos[0], nb.pos[1]
            );
        }
        s.push_str("</g>\n<g>\n");
        for n in &self.nodes {
            let _ = writeln!(
                s,
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/>",
                n.pos[0],
                n.pos[1],
                n.radius,
                n.fill.hex()
            );
        }
        s.push_str("</g>\n");
        if self.labels {
            let _ = writeln!(
                s,
                "<g fill=\"{}\" font-family=\"sans-serif\" font-size=\"{FONT_SIZE}\">",
                p.text.hex()
            );
            for n in &self.nodes {
                let _ = writeln!(
                    s,
                    "<text x=\"{:.2}\" y=\"{:.2}\">{}</text>",
                    n.pos[0] + n.radius + 3.0,
                    n.pos[1] + FONT_SIZE / 3.0,
                    xml_escape(&n.label)
                );
            }
            s.push_str("</g>\n");
        }
        s.push_str("</svg>\n");
        Ok(s)
    }

    /// The scene rasterised at `scale` pixels per layout unit (shrunk so no side exceeds
    /// 8192 px), as PNG bytes. Labels are not drawn.
    ///
    /// # Errors
    /// [`ExportError::Empty`] without nodes, [`ExportError::Png`] when rendering fails.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::many_single_char_names
    )]
    pub fn to_png(&self, scale: f32) -> Result<Vec<u8>, ExportError> {
        let (x, y, w, h) = self.frame().ok_or(ExportError::Empty)?;
        let scale = scale
            .max(0.05)
            .min(MAX_PNG_SIDE / w.max(1.0))
            .min(MAX_PNG_SIDE / h.max(1.0));
        let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
        let mut pixmap = Pixmap::new(pw.max(1), ph.max(1))
            .ok_or_else(|| ExportError::Png("the image size is invalid".to_owned()))?;
        pixmap.fill(self.palette.background.color());
        let transform = Transform::from_translate(-x, -y).post_scale(scale, scale);

        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        let mut edges = PathBuilder::new();
        for &(a, b) in &self.edges {
            let (na, nb) = (&self.nodes[a], &self.nodes[b]);
            edges.move_to(na.pos[0], na.pos[1]);
            edges.line_to(nb.pos[0], nb.pos[1]);
        }
        if let Some(path) = edges.finish() {
            paint.set_color(self.palette.edge.color());
            let stroke = Stroke {
                width: EDGE_WIDTH,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
        for n in &self.nodes {
            let Some(circle) = PathBuilder::from_circle(n.pos[0], n.pos[1], n.radius) else {
                continue;
            };
            paint.set_color(n.fill.color());
            pixmap.fill_path(&circle, &paint, FillRule::Winding, transform, None);
        }
        pixmap
            .encode_png()
            .map_err(|e| ExportError::Png(e.to_string()))
    }
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters other than tab / newline are not valid XML 1.0.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

/// Which format an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Scalable vector graphics.
    Svg,
    /// Raster image.
    Png,
}

impl Format {
    /// File extension.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }

    /// Renders `scene` in this format (PNG at 2x).
    ///
    /// # Errors
    /// See [`ExportError`].
    pub fn render(self, scene: &Scene) -> Result<Vec<u8>, ExportError> {
        match self {
            Self::Svg => scene.to_svg().map(String::into_bytes),
            Self::Png => scene.to_png(2.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            background: Rgb(255, 255, 255),
            edge: Rgb(200, 200, 200),
            node: Rgb(10, 10, 10),
            journal: Rgb(120, 120, 120),
            tag: Rgb(0, 160, 0),
            accent: Rgb(255, 0, 0),
            text: Rgb(0, 0, 0),
        }
    }

    fn node(x: f32, y: f32, label: &str, fill: Rgb) -> SceneNode {
        SceneNode {
            pos: [x, y],
            radius: 8.0,
            label: label.to_owned(),
            fill,
        }
    }

    fn scene(labels: bool) -> Scene {
        Scene {
            nodes: vec![
                node(0.0, 0.0, "Alpha & <Beta>", Rgb(1, 2, 3)),
                node(100.0, 50.0, "Gamma", Rgb(255, 0, 0)),
            ],
            edges: vec![(0, 1)],
            palette: palette(),
            labels,
        }
    }

    #[test]
    fn svg_has_nodes_edges_and_escaped_labels() {
        let svg = scene(true).to_svg().expect("svg");
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert_eq!(svg.matches("<circle").count(), 2);
        assert_eq!(svg.matches("<line").count(), 1);
        assert!(svg.contains("fill=\"#010203\""));
        assert!(svg.contains("Alpha &amp; &lt;Beta&gt;"));
        assert!(svg.trim_end().ends_with("</svg>"));
        // Well-formed enough that every opened group is closed.
        assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count());
    }

    #[test]
    fn labels_are_optional() {
        let svg = scene(false).to_svg().expect("svg");
        assert!(!svg.contains("<text"));
        assert!(!svg.contains("Gamma"));
    }

    #[test]
    fn the_viewbox_contains_every_node() {
        let (x, y, w, h) = scene(false).frame().expect("frame");
        assert!(x <= -8.0 - MARGIN + 0.01 && y <= -8.0 - MARGIN + 0.01);
        assert!(x + w >= 108.0 + MARGIN - 0.01 && y + h >= 58.0 + MARGIN - 0.01);
    }

    #[test]
    fn an_empty_scene_is_refused() {
        let empty = Scene {
            nodes: Vec::new(),
            edges: Vec::new(),
            palette: palette(),
            labels: true,
        };
        assert!(matches!(empty.to_svg(), Err(ExportError::Empty)));
        assert!(matches!(empty.to_png(1.0), Err(ExportError::Empty)));
    }

    #[test]
    fn png_is_a_decodable_image_with_the_background_and_node_colours() {
        let png = scene(false).to_png(1.0).expect("png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = Pixmap::decode_png(&png).expect("decode");
        let (_, _, w, h) = scene(false).frame().expect("frame");
        assert_eq!(pixmap.width(), w.ceil() as u32);
        assert_eq!(pixmap.height(), h.ceil() as u32);
        // Corner = background; the centre of Gamma = its fill.
        let corner = pixmap.pixel(0, 0).expect("corner");
        assert_eq!(
            (corner.red(), corner.green(), corner.blue()),
            (255, 255, 255)
        );
        let (fx, fy, _, _) = scene(false).frame().expect("frame");
        let (cx, cy) = ((100.0 - fx) as u32, (50.0 - fy) as u32);
        let centre = pixmap.pixel(cx, cy).expect("centre");
        assert_eq!((centre.red(), centre.green(), centre.blue()), (255, 0, 0));
    }

    #[test]
    fn png_scale_is_capped() {
        let mut big = scene(false);
        big.nodes[1].pos = [50_000.0, 0.0];
        let png = big.to_png(4.0).expect("png");
        let pixmap = Pixmap::decode_png(&png).expect("decode");
        assert!(pixmap.width() <= MAX_PNG_SIDE as u32);
    }

    #[test]
    fn scene_from_a_model_colours_by_kind() {
        use bitacora_index::{GraphData, GraphDataEdge, GraphDataNode};
        let node = |id: i64, name: &str, is_journal: bool, is_tag: bool| GraphDataNode {
            id,
            name: name.to_owned(),
            degree: 1,
            is_journal,
            is_tag,
            is_namespace_parent: false,
        };
        let data = GraphData {
            nodes: vec![
                node(1, "a", false, false),
                node(2, "b", true, false),
                node(3, "c", false, true),
            ],
            edges: vec![GraphDataEdge { src: 1, dst: 2 }],
        };
        let model = GraphModel::from_data(&data);
        let p = palette();
        let s = Scene::from_model(
            &model,
            &[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]],
            Some(0),
            p,
            true,
        );
        assert_eq!(s.nodes.len(), 3);
        assert_eq!(s.edges.len(), model.links().len());
        let fills: Vec<Rgb> = s.nodes.iter().map(|n| n.fill).collect();
        assert!(fills.contains(&p.accent) && fills.contains(&p.journal) && fills.contains(&p.tag));
    }
}
