//! The graph view (BIT-US-0157, BIT-US-0159): pages as circles sized by their connections, edges
//! for references / tags / namespaces, drawn with GPUI's `canvas()` over a background force
//! simulation (`bitacora-graph`).
//!
//! One view serves both the global graph ([`GraphMode::Global`], a main-area route) and the
//! local graph of a page ([`GraphMode::Local`], a small widget). Rendering is a `canvas()` with
//! `paint_quad` circles and edges batched into a few `PathBuilder` strokes; frames are requested
//! only while the simulation is moving or the user is dragging, so a settled graph costs nothing.
//!
//! Interaction logic (`pointer_down` / `pointer_move` / `pointer_up` / `scroll`) takes positions
//! relative to the canvas centre so it is testable without a window.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bitacora_config::{GraphForce, GraphToggle, GraphViewSettings};
use bitacora_core::queue::CommandQueue;
use bitacora_graph::{Control, ForceParams, Simulation, SimulationHandle};
use bitacora_index::{GraphData, GraphFilter, IndexEvent};
use rust_i18n::t;

use crate::data::GraphHandle;
use crate::graph_view::export::{ExportError, Format, Palette, Rgb, Scene};
use crate::graph_view::prefs::{match_mask, normalize_query, stepped};
use crate::graph_view::viewport::{LABEL_ZOOM, Viewport, hit_test};
use crate::graph_view::{GraphModel, carry_positions};
use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::canvas::{BorderStyle, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder};
use crate::ui::canvas::{ScrollWheelEvent, canvas, quad};
use crate::ui::input::{Input, InputEvent, InputState};
use crate::ui::text_edit::{MouseButton, fill};
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    App, Bounds, Context, Entity, EventEmitter, Hsla, IconName, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Rgba, Sizable as _, Styled as _, Subscription, Task,
    Window, div, h_flex, point, px,
};
use crate::views::kit::{Card, Glyph, IconButton, Segmented};
use crate::views::page_view::PageEvent;
use crate::views::settings::{GraphEdit, edit_config};

mod panel;

/// Debounce between an index change and the graph refresh.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(400);
/// Simulation tick interval (~60 Hz).
const TICK: Duration = Duration::from_millis(16);
/// Alpha a refresh reheats the layout to (BIT-SP-0012.R6).
const REFRESH_ALPHA: f32 = 0.3;
/// Pointer travel (px) under which a press is a click, not a drag.
const CLICK_SLOP: f32 = 3.0;
/// Most labels drawn at once.
const MAX_LABELS: usize = 160;
/// Edges per path: keeps the tessellated vertex count within the `u16` index range.
const EDGES_PER_PATH: usize = 2000;
/// Layout seed (fixed so the same graph lays out the same way).
const SEED: u64 = 0x62_69_74_61;

/// Which graph the view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphMode {
    /// The whole graph (filtered).
    Global,
    /// One page and its 1-hop neighbours.
    Local,
}

/// Settings of the view, derived from `:graph/settings` and `:graph/forcesettings`
/// ([`GraphSettings::from_prefs`]; persisted by [`GraphView::toggle_pref`] and friends).
#[derive(Debug, Clone, Default)]
pub struct GraphSettings {
    /// Which pages are shown.
    pub filter: GraphFilter,
    /// Force parameters of the layout.
    pub forces: ForceParams,
}

impl GraphSettings {
    /// The filter and force parameters of `prefs` (Logseq's `link-dist`, `charge-strength` and
    /// `charge-range` map to the link distance, charge and charge range of the layout).
    #[allow(clippy::cast_possible_truncation)]
    pub fn from_prefs(prefs: &GraphViewSettings) -> Self {
        Self {
            filter: GraphFilter {
                journals: prefs.journals,
                orphans: prefs.orphan_pages,
                builtins: prefs.builtin_pages,
                show_excluded: prefs.excluded_pages,
                ..GraphFilter::default()
            },
            forces: ForceParams {
                link_distance: prefs.link_dist as f32,
                charge: prefs.charge_strength as f32,
                charge_range: prefs.charge_range as f32,
                ..ForceParams::default()
            },
        }
    }
}

/// Sections of the settings panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSection {
    /// Counts and page filters.
    Nodes,
    /// Label search.
    Search,
    /// Layout forces.
    Forces,
    /// Picture export.
    Export,
}

impl PanelSection {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    Node {
        ix: usize,
        moved: bool,
        start: [f32; 2],
        secondary: bool,
    },
    Pan {
        last: [f32; 2],
    },
}

/// Colours resolved from the theme for one frame.
#[derive(Clone, Copy)]
struct Colors {
    bg: Hsla,
    node: Hsla,
    journal: Hsla,
    tag: Hsla,
    accent: Hsla,
    edge: Hsla,
    text: Hsla,
}

/// Everything the paint closure needs (it must be `'static`).
struct PaintData {
    model: Arc<GraphModel>,
    positions: Arc<[[f32; 2]]>,
    viewport: Viewport,
    hover: Option<usize>,
    /// `Some(mask)` while a node is hovered (the node and its neighbours are `true`) or the
    /// panel's label search is active (the matching pages are `true`).
    lit: Option<Vec<bool>>,
    current: Option<usize>,
    focused: Vec<bool>,
    colors: Colors,
}

/// The graph view entity.
pub struct GraphView {
    mode: GraphMode,
    handle: Option<GraphHandle>,
    page: Option<String>,
    settings: GraphSettings,
    /// Everything the index returned (before the focus filter).
    full: GraphModel,
    /// What is laid out and drawn.
    model: Arc<GraphModel>,
    positions: Arc<[[f32; 2]]>,
    sim: Option<SimulationHandle>,
    last_generation: u64,
    /// Frames are requested until the simulation publishes a settled snapshot newer than this.
    wake_generation: Option<u64>,
    settled: bool,
    viewport: Viewport,
    user_moved: bool,
    bounds: Rc<Cell<Bounds<crate::ui::Pixels>>>,
    hover: Option<usize>,
    drag: Option<Drag>,
    current_id: Option<i64>,
    focus: Vec<i64>,
    hops: u32,
    loaded: bool,
    load_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    // ---- settings panel (BIT-US-0158) ----
    prefs: GraphViewSettings,
    /// The graph `prefs` were read for (they are read again only when the graph changes).
    prefs_root: Option<std::path::PathBuf>,
    queue: Option<CommandQueue>,
    panel_open: bool,
    sections: [bool; 4],
    paused: bool,
    query: String,
    search: Option<(Entity<InputState>, Subscription)>,
    message: Option<String>,
    /// Config edits waiting for the write in flight to finish.
    pending: Vec<GraphEdit>,
    saving: bool,
}

impl std::fmt::Debug for GraphView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphView")
            .field("mode", &self.mode)
            .field("nodes", &self.model.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PageEvent> for GraphView {}

impl GraphView {
    /// An empty view; nothing loads until [`GraphView::show`].
    pub fn new(mode: GraphMode) -> Self {
        Self {
            mode,
            handle: None,
            page: None,
            settings: GraphSettings::default(),
            full: GraphModel::default(),
            model: Arc::new(GraphModel::default()),
            positions: Arc::from(Vec::new()),
            sim: None,
            last_generation: 0,
            wake_generation: None,
            settled: true,
            viewport: Viewport::default(),
            user_moved: false,
            bounds: Rc::new(Cell::new(Bounds::default())),
            hover: None,
            drag: None,
            current_id: None,
            focus: Vec::new(),
            hops: 1,
            loaded: false,
            load_task: None,
            refresh_task: None,
            prefs: GraphViewSettings::default(),
            prefs_root: None,
            queue: None,
            panel_open: false,
            sections: [true, false, true, false],
            paused: false,
            query: String::new(),
            search: None,
            message: None,
            pending: Vec::new(),
            saving: false,
        }
    }

    /// Connects the view to a graph and loads it.
    pub fn show(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        // The global graph follows the saved settings; they are read once per graph (edits made
        // here are kept in `prefs`, the handle only has the config as it was when it opened).
        if self.mode == GraphMode::Global && self.prefs_root.as_deref() != Some(&handle.root) {
            self.prefs = handle.settings.config.graph_view_settings();
            self.prefs_root = Some(handle.root.clone());
            self.settings = GraphSettings::from_prefs(&self.prefs);
        }
        self.handle = Some(handle);
        self.reload(cx);
    }

    /// Gives the view the command queue its settings edits are written through.
    pub fn set_queue(&mut self, queue: Option<CommandQueue>) {
        self.queue = queue;
    }

    /// Forgets the graph (it was closed).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.sim = None;
        self.full = GraphModel::default();
        self.model = Arc::new(GraphModel::default());
        self.positions = Arc::from(Vec::new());
        self.loaded = false;
        self.focus.clear();
        self.prefs_root = None;
        self.queue = None;
        cx.notify();
    }

    /// Replaces the settings and reloads (seam for BIT-US-0158).
    pub fn set_settings(&mut self, settings: GraphSettings, cx: &mut Context<Self>) {
        self.settings = settings;
        self.reload(cx);
    }

    /// The local graph follows this page (local mode only).
    pub fn set_page(&mut self, page: Option<String>, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.user_moved = false;
            self.reload(cx);
        }
    }

    /// Whether the data was loaded at least once.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The nodes laid out and drawn.
    pub fn model(&self) -> &GraphModel {
        &self.model
    }

    /// Current positions (layout space).
    pub fn positions(&self) -> &[[f32; 2]] {
        &self.positions
    }

    /// The pan / zoom transform.
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// The hovered node.
    pub fn hover(&self) -> Option<usize> {
        self.hover
    }

    /// Page ids the view is focused on (empty = whole graph).
    pub fn focus(&self) -> &[i64] {
        &self.focus
    }

    /// Hops shown around the focus nodes.
    pub fn hops(&self) -> u32 {
        self.hops
    }

    /// The layout has stopped moving.
    pub fn is_settled(&self) -> bool {
        self.settled
    }

    /// Whether the view still wants animation frames.
    pub fn wants_frames(&self) -> bool {
        !self.settled || self.drag.is_some() || self.wake_generation.is_some()
    }

    // ---- data ---------------------------------------------------------------------------

    /// Reads the graph from the index on a background thread.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let filter = self.settings.filter.clone();
        let mode = self.mode;
        let page = self.page.clone();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load(&handle, mode, page.as_deref(), &filter) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok((data, current)) => view.apply_data(&data, current, cx),
                Err(message) => tracing::warn!("cannot load the graph: {message}"),
            });
        }));
    }

    /// Any index change may add, remove or relink pages: refresh (debounced) keeping positions.
    pub fn on_index_event(&mut self, _: &IndexEvent, cx: &mut Context<Self>) {
        if self.handle.is_none() {
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| view.reload(cx));
        }));
    }

    /// Installs freshly loaded data: diffs against the current layout, keeps the positions of
    /// surviving pages, seeds new ones next to a neighbour and reheats gently.
    pub fn apply_data(&mut self, data: &GraphData, current: Option<i64>, cx: &mut Context<Self>) {
        self.loaded = true;
        self.current_id = current;
        self.full = GraphModel::from_data(data);
        self.rebuild(cx);
        cx.notify();
    }

    /// The model that should be drawn given the focus.
    fn shown_model(&self) -> GraphModel {
        if self.focus.is_empty() {
            return self.full.clone();
        }
        self.full
            .restrict(&self.full.within_hops(&self.focus, self.hops))
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        // Focus pages that vanished from the graph drop out of the focus.
        let full = &self.full;
        self.focus.retain(|id| full.index_of(*id).is_some());
        let shown = self.shown_model();
        if self.sim.is_some() && shown == *self.model {
            return;
        }
        let first = self.sim.is_none();
        let initial = carry_positions(&self.model, &self.positions, &shown);
        let alpha = if first { 1.0 } else { REFRESH_ALPHA };
        let simulation = Simulation::with_positions(
            &shown.to_input(),
            self.settings.forces,
            SEED,
            &initial,
            alpha,
        );
        self.positions = simulation.snapshot();
        self.sim = SimulationHandle::spawn(simulation, TICK);
        self.last_generation = 0;
        self.settled = self.sim.is_none();
        self.wake_generation = None;
        self.hover = None;
        self.drag = None;
        self.model = Arc::new(shown);
        cx.notify();
    }

    // ---- focus (BIT-US-0159) -------------------------------------------------------------

    /// Adds `id` to the focus, or removes it when already focused.
    pub fn toggle_focus(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(at) = self.focus.iter().position(|f| *f == id) {
            self.focus.remove(at);
        } else {
            self.focus.push(id);
        }
        self.after_focus_change(cx);
    }

    /// Shows the whole graph again.
    pub fn reset_focus(&mut self, cx: &mut Context<Self>) {
        self.focus.clear();
        self.after_focus_change(cx);
    }

    /// Sets how many hops around the focus nodes stay visible (1..=6).
    pub fn set_hops(&mut self, hops: u32, cx: &mut Context<Self>) {
        let hops = hops.clamp(1, 6);
        if hops != self.hops {
            self.hops = hops;
            self.after_focus_change(cx);
        }
    }

    fn after_focus_change(&mut self, cx: &mut Context<Self>) {
        self.rebuild(cx);
        cx.notify();
    }

    // ---- interaction ---------------------------------------------------------------------

    fn send(&mut self, control: Control) {
        if let Some(sim) = &self.sim {
            self.wake_generation = Some(sim.snapshot().generation);
            sim.send(control);
        }
    }

    /// Left button pressed at `pos` (canvas-centre-relative px).
    pub fn pointer_down(&mut self, pos: [f32; 2], secondary: bool, cx: &mut Context<Self>) {
        self.drag = Some(
            match hit_test(&self.model, &self.positions, &self.viewport, pos) {
                Some(ix) => Drag::Node {
                    ix,
                    moved: false,
                    start: pos,
                    secondary,
                },
                None => Drag::Pan { last: pos },
            },
        );
        cx.notify();
    }

    /// Pointer moved to `pos`.
    pub fn pointer_move(&mut self, pos: [f32; 2], cx: &mut Context<Self>) {
        match self.drag {
            Some(Drag::Pan { last }) => {
                self.viewport.pan(pos[0] - last[0], pos[1] - last[1]);
                self.drag = Some(Drag::Pan { last: pos });
                self.user_moved = true;
                cx.notify();
            }
            Some(Drag::Node {
                ix,
                moved,
                start,
                secondary,
            }) => {
                let travelled = (pos[0] - start[0]).hypot(pos[1] - start[1]);
                if moved || travelled > CLICK_SLOP {
                    self.drag = Some(Drag::Node {
                        ix,
                        moved: true,
                        start,
                        secondary,
                    });
                    self.user_moved = true;
                    let at = self.viewport.to_world(pos);
                    // Keep the node visually where the pointer is, even before the next tick.
                    let mut moved_to = self.positions.to_vec();
                    if let Some(p) = moved_to.get_mut(ix) {
                        *p = at;
                    }
                    self.positions = moved_to.into();
                    self.send(Control::Pin { node: ix, at });
                    cx.notify();
                }
            }
            None => {
                let hover = hit_test(&self.model, &self.positions, &self.viewport, pos);
                if hover != self.hover {
                    self.hover = hover;
                    cx.notify();
                }
            }
        }
    }

    /// Left button released at `pos`: ends a drag, or counts as a click on a node.
    pub fn pointer_up(&mut self, shift: bool, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if let Drag::Node {
            ix,
            moved,
            secondary,
            ..
        } = drag
        {
            if moved {
                self.send(Control::Unpin(ix));
            } else if let Some(node) = self.model.nodes().get(ix) {
                if secondary {
                    let id = node.id;
                    self.toggle_focus(id, cx);
                } else {
                    let name = node.name.clone();
                    cx.emit(PageEvent::open(
                        NavTarget::Page(name),
                        OpenIn::from_shift(shift),
                    ));
                }
            }
        }
        cx.notify();
    }

    /// Wheel at `pos` by `dy` pixels (up zooms in).
    pub fn scroll(&mut self, pos: [f32; 2], dy: f32, cx: &mut Context<Self>) {
        self.viewport.zoom_at(pos, (dy * 0.0015).exp());
        self.user_moved = true;
        cx.notify();
    }

    /// Fits the whole layout in the canvas again.
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.user_moved = false;
        self.fit_now();
        cx.notify();
    }

    fn fit_now(&mut self) {
        let b = self.bounds.get();
        let size = [f32::from(b.size.width), f32::from(b.size.height)];
        self.viewport = Viewport::fit(&self.positions, size);
    }

    /// Pulls the newest layout snapshot; returns whether frames are still needed.
    pub(crate) fn sync_snapshot(&mut self) {
        let Some(sim) = &self.sim else {
            self.settled = true;
            return;
        };
        let snap = sim.snapshot();
        let changed = snap.generation != self.last_generation;
        // While a node is dragged its position is driven by the pointer, not the snapshot.
        if changed {
            self.last_generation = snap.generation;
            self.positions = snap.positions.clone();
        }
        self.settled = snap.settled;
        if let Some(g) = self.wake_generation
            && snap.generation > g
            && snap.settled
        {
            self.wake_generation = None;
        }
        if changed && !self.user_moved {
            self.fit_now();
        }
    }

    fn colors(cx: &App) -> Colors {
        let c = &cx.bitacora().colors;
        Colors {
            bg: c.bg,
            node: c.text_2,
            journal: c.muted,
            tag: c.ok,
            accent: c.accent,
            edge: c.line_2,
            text: c.text,
        }
    }

    /// `Some(mask)` while the panel's label search has text: `true` for the matching pages.
    pub(crate) fn search_mask(&self) -> Option<Vec<bool>> {
        let q = normalize_query(&self.query)?;
        Some(match_mask(
            self.model.nodes().iter().map(|n| n.name.as_str()),
            &q,
        ))
    }

    fn paint_data(&self, cx: &App) -> PaintData {
        let lit = self.hover.map(|h| {
            let mut mask = vec![false; self.model.len()];
            if let Some(slot) = mask.get_mut(h) {
                *slot = true;
            }
            for &nb in self.model.neighbours(h) {
                if let Some(slot) = mask.get_mut(nb as usize) {
                    *slot = true;
                }
            }
            mask
        });
        let lit = lit.or_else(|| self.search_mask());
        let mut focused = vec![false; self.model.len()];
        for id in &self.focus {
            if let Some(slot) = self.model.index_of(*id).and_then(|i| focused.get_mut(i)) {
                *slot = true;
            }
        }
        PaintData {
            model: self.model.clone(),
            positions: self.positions.clone(),
            viewport: self.viewport,
            hover: self.hover,
            lit,
            current: self.current_id.and_then(|id| self.model.index_of(id)),
            focused,
            colors: Self::colors(cx),
        }
    }

    /// Indices of the nodes whose label is drawn.
    fn label_nodes(&self, size: [f32; 2]) -> Vec<usize> {
        let mut out = Vec::new();
        let always = |i: usize| {
            Some(i) == self.hover
                || self
                    .hover
                    .is_some_and(|h| self.model.neighbours(h).contains(&(i as u32)))
                || self.current_id.and_then(|id| self.model.index_of(id)) == Some(i)
        };
        for (i, p) in self.positions.iter().enumerate() {
            let s = self.viewport.to_screen(*p);
            if !Viewport::disc_visible(s, 0.0, size) {
                continue;
            }
            if self.viewport.zoom >= LABEL_ZOOM || always(i) {
                out.push(i);
                if out.len() >= MAX_LABELS {
                    break;
                }
            }
        }
        out
    }
}

/// Loads the graph (or the page's neighbourhood) and the id of the page to highlight.
fn load(
    handle: &GraphHandle,
    mode: GraphMode,
    page: Option<&str>,
    filter: &GraphFilter,
) -> Result<(GraphData, Option<i64>), String> {
    let err = |e: bitacora_index::Error| e.to_string();
    match mode {
        GraphMode::Global => Ok((handle.reader.graph_data(filter).map_err(err)?, None)),
        GraphMode::Local => {
            let Some(name) = page else {
                return Ok((GraphData::default(), None));
            };
            let Some(id) = handle.reader.page_id(name).map_err(err)? else {
                return Ok((GraphData::default(), None));
            };
            Ok((
                handle.reader.local_graph_data(id, filter).map_err(err)?,
                Some(id),
            ))
        }
    }
}

/// Buffers line segments and strokes them as few paths of one colour.
struct EdgeBatch(
    Vec<(
        crate::ui::Point<crate::ui::Pixels>,
        crate::ui::Point<crate::ui::Pixels>,
    )>,
);

impl EdgeBatch {
    fn flush(&mut self, color: Hsla, window: &mut Window) {
        for chunk in self.0.chunks(EDGES_PER_PATH) {
            let mut builder = PathBuilder::stroke(px(1.0));
            for (a, b) in chunk {
                builder.move_to(*a);
                builder.line_to(*b);
            }
            match builder.build() {
                Ok(path) => window.paint_path(path, color),
                Err(error) => tracing::debug!("cannot stroke graph edges: {error}"),
            }
        }
        self.0.clear();
    }
}

fn paint_graph(bounds: Bounds<crate::ui::Pixels>, d: &PaintData, window: &mut Window) {
    let size = [f32::from(bounds.size.width), f32::from(bounds.size.height)];
    let origin = [
        f32::from(bounds.origin.x) + size[0] / 2.0,
        f32::from(bounds.origin.y) + size[1] / 2.0,
    ];
    let pt = |s: [f32; 2]| point(px(origin[0] + s[0]), px(origin[1] + s[1]));
    window.paint_quad(fill(bounds, d.colors.bg));
    let screen: Vec<[f32; 2]> = d
        .positions
        .iter()
        .map(|p| d.viewport.to_screen(*p))
        .collect();

    // Edges: one batch per colour.
    let dim = d.lit.is_some();
    let mut plain = EdgeBatch(Vec::new());
    let mut lit = EdgeBatch(Vec::new());
    for &(a, b) in d.model.links() {
        let (Some(&sa), Some(&sb)) = (screen.get(a as usize), screen.get(b as usize)) else {
            continue;
        };
        if !Viewport::segment_visible(sa, sb, size) {
            continue;
        }
        let touches_hover = d.hover.is_some_and(|h| h == a as usize || h == b as usize);
        if touches_hover {
            lit.0.push((pt(sa), pt(sb)));
        } else {
            plain.0.push((pt(sa), pt(sb)));
        }
    }
    plain.flush(
        if dim {
            d.colors.edge.opacity(0.35)
        } else {
            d.colors.edge
        },
        window,
    );
    lit.flush(d.colors.accent, window);

    // Nodes: dimmed ones first so highlighted ones are on top.
    for pass in 0..2 {
        for (i, node) in d.model.nodes().iter().enumerate() {
            let Some(&s) = screen.get(i) else { break };
            let r = (node.radius() * d.viewport.zoom).max(2.0);
            if !Viewport::disc_visible(s, r, size) {
                continue;
            }
            let bright = d
                .lit
                .as_ref()
                .is_none_or(|m| m.get(i).copied().unwrap_or(false));
            if (pass == 0) == bright {
                continue;
            }
            let base = if Some(i) == d.current || d.hover == Some(i) {
                d.colors.accent
            } else if node.is_tag {
                d.colors.tag
            } else if node.is_journal {
                d.colors.journal
            } else {
                d.colors.node
            };
            let color = if bright { base } else { base.opacity(0.25) };
            let ring = d.focused.get(i).copied().unwrap_or(false);
            let rect = Bounds::new(
                pt([s[0] - r, s[1] - r]),
                crate::ui::size(px(2.0 * r), px(2.0 * r)),
            );
            window.paint_quad(quad(
                rect,
                px(r),
                color,
                if ring { px(2.0) } else { px(0.0) },
                d.colors.text,
                BorderStyle::default(),
            ));
        }
    }
}

impl Render for GraphView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_snapshot();
        if self.wants_frames() {
            window.request_animation_frame();
        }
        let theme = cx.bitacora().clone();
        let colors = Self::colors(cx);
        let data = self.paint_data(cx);
        let bounds = self.bounds.clone();
        let b = bounds.get();
        let size = [f32::from(b.size.width), f32::from(b.size.height)];

        let rel = {
            let bounds = bounds.clone();
            move |p: crate::ui::Point<crate::ui::Pixels>| -> [f32; 2] {
                let b = bounds.get();
                [
                    f32::from(p.x) - f32::from(b.origin.x) - f32::from(b.size.width) / 2.0,
                    f32::from(p.y) - f32::from(b.origin.y) - f32::from(b.size.height) / 2.0,
                ]
            }
        };

        let mut labels = div().absolute().size_full();
        if size[0] > 1.0 {
            for i in self.label_nodes(size) {
                let (Some(node), Some(p)) = (self.model.nodes().get(i), self.positions.get(i))
                else {
                    continue;
                };
                let s = self.viewport.to_screen(*p);
                let r = (node.radius() * self.viewport.zoom).max(2.0);
                labels = labels.child(
                    div()
                        .absolute()
                        .left(px(size[0] / 2.0 + s[0] + r + 3.0))
                        .top(px(size[1] / 2.0 + s[1] - 8.0))
                        .text_xs()
                        .text_color(colors.text)
                        .child(node.name.clone()),
                );
            }
        }

        let (down, moving, wheel) = (rel.clone(), rel.clone(), rel);
        let mut root = div()
            .id("graph-view")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.bg)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.pointer_down(down(e.position), e.modifiers.secondary(), cx);
                }),
            )
            .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, _, cx| {
                if this.drag.is_some() && e.pressed_button != Some(MouseButton::Left) {
                    this.pointer_up(false, cx);
                }
                this.pointer_move(moving(e.position), cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseUpEvent, _, cx| {
                    this.pointer_up(e.modifiers.shift, cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.pointer_up(false, cx)),
            )
            .on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, window, cx| {
                let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                this.scroll(wheel(e.position), dy, cx);
            }))
            .child(
                canvas(
                    move |b, _, _| {
                        bounds.set(b);
                        b
                    },
                    move |b, _, window, _| paint_graph(b, &data, window),
                )
                .absolute()
                .size_full(),
            )
            .child(labels);

        if self.mode == GraphMode::Global {
            root = root.child(self.toolbar(cx));
            if self.panel_open {
                root = root.child(self.panel(window, cx));
            }
        }
        if self.loaded && self.model.is_empty() {
            root = root.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.colors.muted)
                    .child(t!("graph_view.empty").to_string()),
            );
        }
        root
    }
}

impl GraphView {
    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Self::colors(cx);
        let panel = cx.bitacora().colors.panel;
        let line = cx.bitacora().colors.line;
        let this = cx.entity();
        let (fit, less, more, reset, settings) =
            (this.clone(), this.clone(), this.clone(), this.clone(), this);
        let panel_open = self.panel_open;
        let hops = self.hops;
        let mut bar = h_flex()
            .absolute()
            .top(px(12.0))
            .left(px(12.0))
            .gap_2()
            .items_center()
            .px(px(8.0))
            .py(px(4.0))
            .bg(panel)
            .border_1()
            .border_color(line)
            .rounded(px(6.0))
            .text_sm()
            .text_color(colors.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(t!("graph_view.pages", count = self.model.len()).to_string())
            .child(
                Button::new("graph-fit")
                    .ghost()
                    .small()
                    .label(t!("graph_view.fit").to_string())
                    .on_click(move |_, _, cx| fit.update(cx, |v, cx| v.fit(cx))),
            )
            .child(
                IconButton::new("graph-settings", Glyph::Settings)
                    .small()
                    .active(panel_open)
                    .on_click(move |_, _, cx| settings.update(cx, |v, cx| v.toggle_panel(cx))),
            );
        if !self.focus.is_empty() {
            bar = bar
                .child(t!("graph_view.hops", n = hops).to_string())
                .child(
                    Button::new("graph-hops-less")
                        .ghost()
                        .small()
                        .icon(IconName::Minus)
                        .tooltip(t!("graph_view.less").to_string())
                        .on_click(move |_, _, cx| {
                            less.update(cx, |v, cx| {
                                let n = v.hops.saturating_sub(1);
                                v.set_hops(n, cx);
                            });
                        }),
                )
                .child(
                    Button::new("graph-hops-more")
                        .ghost()
                        .small()
                        .icon(IconName::Plus)
                        .tooltip(t!("graph_view.more").to_string())
                        .on_click(move |_, _, cx| {
                            more.update(cx, |v, cx| {
                                let n = v.hops + 1;
                                v.set_hops(n, cx);
                            });
                        }),
                )
                .child(
                    Button::new("graph-reset-focus")
                        .ghost()
                        .small()
                        .label(t!("graph_view.reset_focus").to_string())
                        .on_click(move |_, _, cx| reset.update(cx, |v, cx| v.reset_focus(cx))),
                );
        }
        bar
    }
}
