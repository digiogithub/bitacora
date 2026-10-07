//! The graph settings panel (BIT-US-0158) and picture export (BIT-US-0160).
//!
//! Settings live in `logseq/config.edn` (`:graph/settings`, `:graph/forcesettings`, Logseq
//! 0.10.x keys). Every change updates the view at once and is written through the command
//! queue ([`edit_config`]: comment-preserving, atomic, hash-checked). Writes are serialised: while
//! one is in flight, further edits wait in `pending` and go out as one batch.

use crate::views::dims;
use std::path::{Path, PathBuf};

use super::{
    Card, Entity, ExportError, Format, GraphEdit, GraphForce, GraphToggle, Input, InputEvent,
    InputState, Palette, Rgb, Rgba, Scene, Segmented, edit_config, stepped,
};
use super::{GraphSettings, GraphView, PanelSection, Relayout};
use bitacora_graph::Control;
use rust_i18n::t;

use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    App, AppContext as _, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, h_flex,
};
use crate::views::kit::{Button, Glyph};

/// Blends `color` over `base` and rounds to 8-bit sRGB.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn flatten(color: Hsla, base: Rgb) -> Rgb {
    let c: Rgba = color.into();
    let mix = |fg: f32, bg: u8| {
        let v = fg * c.a + (f32::from(bg) / 255.0) * (1.0 - c.a);
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Rgb(mix(c.r, base.0), mix(c.g, base.1), mix(c.b, base.2))
}

/// Renders `scene` and writes it atomically to `path`.
///
/// # Errors
/// A user-presentable message.
pub fn write_export(scene: &Scene, format: Format, path: &Path) -> Result<(), String> {
    let bytes = format
        .render(scene)
        .map_err(|e: ExportError| e.to_string())?;
    crate::settings::write_atomic(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// `path` with the format's extension when it has none.
fn with_extension(mut path: PathBuf, format: Format) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(format.extension());
    }
    path
}

impl GraphView {
    /// The saved-and-applied settings of the panel.
    pub fn prefs(&self) -> &bitacora_config::GraphViewSettings {
        &self.prefs
    }

    /// Whether the panel is showing.
    pub fn panel_open(&self) -> bool {
        self.panel_open
    }

    /// The last status or error line of the panel.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Whether a config write is in flight or queued.
    pub fn is_saving(&self) -> bool {
        self.saving || !self.pending.is_empty()
    }

    /// Whether the layout simulation is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// The label search text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Opens or closes the panel.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        self.panel_open = !self.panel_open;
        cx.notify();
    }

    /// Expands or collapses a section.
    pub fn toggle_section(&mut self, section: PanelSection, cx: &mut Context<Self>) {
        let open = &mut self.sections[section.index()];
        *open = !*open;
        cx.notify();
    }

    /// Whether `section` is expanded.
    pub fn section_open(&self, section: PanelSection) -> bool {
        self.sections[section.index()]
    }

    /// Sets a `:graph/settings` filter, saves it and reloads the graph.
    pub fn toggle_pref(&mut self, toggle: GraphToggle, value: bool, cx: &mut Context<Self>) {
        if self.prefs.toggle(toggle) == value {
            return;
        }
        self.prefs.set_toggle(toggle, value);
        self.persist(GraphEdit::GraphToggle(toggle, value), cx);
        self.set_settings(GraphSettings::from_prefs(&self.prefs), cx);
    }

    /// Sets a `:graph/forcesettings` value, saves it and re-lays the graph out.
    #[allow(clippy::cast_precision_loss)]
    pub fn set_force(&mut self, force: GraphForce, value: i64, cx: &mut Context<Self>) {
        #[allow(clippy::float_cmp)]
        if self.prefs.force(force) == value as f64 {
            return;
        }
        self.prefs.set_force(force, value as f64);
        self.persist(GraphEdit::GraphForce(force, value), cx);
        self.apply_forces(cx);
    }

    /// Moves a force one slider step (`dir` > 0 up, otherwise down).
    pub fn step_force(&mut self, force: GraphForce, dir: i32, cx: &mut Context<Self>) {
        let next = stepped(force, self.prefs.force(force), dir);
        self.set_force(force, next, cx);
    }

    /// Restores Logseq's default forces and removes `:graph/forcesettings`.
    pub fn reset_forces(&mut self, cx: &mut Context<Self>) {
        let defaults = bitacora_config::GraphViewSettings::default();
        let changed = GraphForce::ALL
            .iter()
            .any(|f| (self.prefs.force(*f) - defaults.force(*f)).abs() > f64::EPSILON);
        for f in GraphForce::ALL {
            self.prefs.set_force(f, defaults.force(f));
        }
        self.persist(GraphEdit::GraphForcesReset, cx);
        if changed {
            self.apply_forces(cx);
        }
    }

    /// Starts a new layout with the current force settings, keeping the positions.
    fn apply_forces(&mut self, cx: &mut Context<Self>) {
        self.settings.forces = GraphSettings::from_prefs(&self.prefs).forces;
        self.rebuild(Relayout::Params, cx);
    }

    /// Pauses or resumes the layout simulation.
    pub fn set_paused(&mut self, paused: bool, cx: &mut Context<Self>) {
        self.paused = paused;
        self.send(Control::Pause(paused));
        cx.notify();
    }

    /// Sets the label search (matching pages stay bright, the rest dims).
    pub fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.query = query.to_owned();
        cx.notify();
    }

    fn persist(&mut self, edit: GraphEdit, cx: &mut Context<Self>) {
        self.pending.push(edit);
        self.flush(cx);
    }

    /// Writes the queued edits, one batch at a time.
    fn flush(&mut self, cx: &mut Context<Self>) {
        if self.saving || self.pending.is_empty() {
            return;
        }
        let (Some(queue), Some(root)) = (self.queue.clone(), self.prefs_root.clone()) else {
            // No live session yet (or a test without one): nothing to write to.
            self.pending.clear();
            return;
        };
        let edits = std::mem::take(&mut self.pending);
        self.saving = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { edit_config(&queue, &root, &edits) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.saving = false;
                if let Err(error) = result {
                    view.message =
                        Some(t!("graph_view.save_failed", error = error.to_string()).to_string());
                    cx.notify();
                }
                view.flush(cx);
            });
        })
        .detach();
    }

    // ---- export (BIT-US-0160) --------------------------------------------------------------

    fn palette(cx: &App) -> Palette {
        let c = &cx.bitacora().colors;
        let background = flatten(c.bg, Rgb(255, 255, 255));
        Palette {
            background,
            edge: flatten(c.line_2, background),
            node: flatten(c.text_2, background),
            journal: flatten(c.muted, background),
            tag: flatten(c.ok, background),
            accent: flatten(c.accent, background),
            text: flatten(c.text, background),
        }
    }

    /// The scene an export would draw: the pages and edges on screen, at their current layout
    /// positions, in the theme colours.
    pub fn scene(&self, labels: bool, cx: &App) -> Scene {
        let current = self.current_id.and_then(|id| self.model.index_of(id));
        Scene::from_model(
            &self.model,
            &self.positions,
            current,
            Self::palette(cx),
            labels,
        )
    }

    /// Asks where to save and writes the picture (SVG with labels, PNG without).
    pub fn export(&mut self, format: Format, cx: &mut Context<Self>) {
        if self.model.is_empty() {
            self.message = Some(ExportError::Empty.to_string());
            cx.notify();
            return;
        }
        let scene = self.scene(format == Format::Svg, cx);
        let dir = directories::UserDirs::new()
            .and_then(|d| d.document_dir().map(Path::to_path_buf))
            .or_else(|| self.handle.as_ref().map(|h| h.root.clone()))
            .unwrap_or_default();
        let name = format!("graph.{}", format.extension());
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            // A cancelled dialog, a closed channel or a portal error all mean "nothing chosen".
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let path = with_extension(path, format);
            let target = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { write_export(&scene, format, &target) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.message = Some(match result {
                    Ok(()) => {
                        t!("graph_view.exported", path = path.display().to_string()).to_string()
                    }
                    Err(error) => t!("graph_view.export_failed", error = error).to_string(),
                });
                cx.notify();
            });
        })
        .detach();
    }

    // ---- rendering -------------------------------------------------------------------------

    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some((input, _)) = &self.search {
            return input.clone();
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("graph_view.search_placeholder").to_string())
        });
        let sub = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value().to_string();
                this.set_query(&text, cx);
            }
        });
        self.search = Some((input.clone(), sub));
        input
    }

    fn section_header(
        &self,
        section: PanelSection,
        title: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.section_open(section);
        let this = cx.entity();
        Button::new(("graph-section", section.index()))
            .ghost()
            .compact()
            .icon(if open {
                Glyph::ChevronDown
            } else {
                Glyph::ChevronRight
            })
            .label(title)
            .on_click(move |_, _, cx| this.update(cx, |v, cx| v.toggle_section(section, cx)))
    }

    fn toggle_row(
        &self,
        toggle: GraphToggle,
        label: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on = self.prefs.toggle(toggle);
        let this = cx.entity();
        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(label)
            .child(
                Segmented::new(("graph-toggle", toggle as usize))
                    .option("on", t!("graph_view.on").to_string())
                    .option("off", t!("graph_view.off").to_string())
                    .selected(if on { "on" } else { "off" })
                    .on_change(move |key, _, cx| {
                        let value = key.as_ref() == "on";
                        this.update(cx, |v, cx| v.toggle_pref(toggle, value, cx));
                    }),
            )
    }

    fn force_row(
        &self,
        force: GraphForce,
        label: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let value = self.prefs.force(force);
        let (less, more) = (cx.entity(), cx.entity());
        let id = force as usize;
        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(label)
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new(("graph-force-less", id))
                            .compact()
                            .label(t!("graph_view.minus").to_string())
                            .on_click(move |_, _, cx| {
                                less.update(cx, |v, cx| v.step_force(force, -1, cx));
                            }),
                    )
                    .child(
                        div()
                            .min_w(dims::PX_44)
                            .text_center()
                            .child(format!("{value:.0}")),
                    )
                    .child(
                        Button::new(("graph-force-more", id))
                            .compact()
                            .label(t!("graph_view.plus").to_string())
                            .on_click(move |_, _, cx| {
                                more.update(cx, |v, cx| v.step_force(force, 1, cx));
                            }),
                    ),
            )
    }

    /// The floating panel: Nodes, Search, Forces and Export sections.
    pub(super) fn panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.bitacora().colors;
        let search = self.ensure_search(window, cx);
        let this = cx.entity();
        let mut body = div().flex().flex_col().gap_2();

        body = body.child(self.section_header(
            PanelSection::Nodes,
            t!("graph_view.nodes").to_string(),
            cx,
        ));
        if self.section_open(PanelSection::Nodes) {
            let paused = self.paused;
            let pause = this.clone();
            body = body
                .child(div().text_color(colors.muted).child(format!(
                    "{}, {}",
                    t!("graph_view.pages", count = self.model.len()),
                    t!("graph_view.links", count = self.model.links().len())
                )))
                .child(self.toggle_row(
                    GraphToggle::Journals,
                    t!("graph_view.journals").to_string(),
                    cx,
                ))
                .child(self.toggle_row(
                    GraphToggle::OrphanPages,
                    t!("graph_view.orphans").to_string(),
                    cx,
                ))
                .child(self.toggle_row(
                    GraphToggle::BuiltinPages,
                    t!("graph_view.builtins").to_string(),
                    cx,
                ))
                .child(self.toggle_row(
                    GraphToggle::ExcludedPages,
                    t!("graph_view.excluded").to_string(),
                    cx,
                ))
                .child(
                    Button::new("graph-pause")
                        .secondary()
                        .compact()
                        .label(if paused {
                            t!("graph_view.resume").to_string()
                        } else {
                            t!("graph_view.pause").to_string()
                        })
                        .on_click(move |_, _, cx| {
                            pause.update(cx, |v, cx| {
                                let next = !v.paused;
                                v.set_paused(next, cx);
                            });
                        }),
                );
        }

        body = body.child(self.section_header(
            PanelSection::Search,
            t!("graph_view.search").to_string(),
            cx,
        ));
        if self.section_open(PanelSection::Search) {
            body = body.child(Input::new(&search));
        }

        body = body.child(self.section_header(
            PanelSection::Forces,
            t!("graph_view.forces").to_string(),
            cx,
        ));
        if self.section_open(PanelSection::Forces) {
            let reset = this.clone();
            body = body
                .child(self.force_row(
                    GraphForce::LinkDist,
                    t!("graph_view.link_dist").to_string(),
                    cx,
                ))
                .child(self.force_row(
                    GraphForce::ChargeStrength,
                    t!("graph_view.charge").to_string(),
                    cx,
                ))
                .child(self.force_row(
                    GraphForce::ChargeRange,
                    t!("graph_view.charge_range").to_string(),
                    cx,
                ))
                .child(
                    Button::new("graph-reset-forces")
                        .ghost()
                        .compact()
                        .label(t!("graph_view.reset_forces").to_string())
                        .on_click(move |_, _, cx| reset.update(cx, |v, cx| v.reset_forces(cx))),
                );
        }

        body = body.child(self.section_header(
            PanelSection::Export,
            t!("graph_view.export").to_string(),
            cx,
        ));
        if self.section_open(PanelSection::Export) {
            let (svg, png) = (this.clone(), this);
            body = body
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("graph-export-svg")
                                .secondary()
                                .compact()
                                .label(t!("graph_view.export_svg").to_string())
                                .on_click(move |_, _, cx| {
                                    svg.update(cx, |v, cx| v.export(Format::Svg, cx));
                                }),
                        )
                        .child(
                            Button::new("graph-export-png")
                                .secondary()
                                .compact()
                                .label(t!("graph_view.export_png").to_string())
                                .on_click(move |_, _, cx| {
                                    png.update(cx, |v, cx| v.export(Format::Png, cx));
                                }),
                        ),
                )
                .child(
                    div()
                        .text_color(colors.muted)
                        .child(t!("graph_view.export_note").to_string()),
                );
        }
        if let Some(message) = &self.message {
            body = body.child(div().text_color(colors.muted).child(message.clone()));
        }

        div()
            .id("graph-panel")
            .absolute()
            .top(dims::PX_12)
            .right(dims::PX_12)
            .w(dims::PX_300)
            .max_h_full()
            .overflow_y_scroll()
            .text_sm()
            .text_color(colors.text)
            // The canvas underneath pans, zooms and selects: keep those gestures out of here.
            .on_mouse_down(super::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(Card::new().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_view::export::SceneNode;

    #[test]
    fn exports_are_written_atomically_with_the_format_extension() {
        let dir = tempfile::tempdir().expect("tempdir");
        let scene = Scene {
            nodes: vec![SceneNode {
                pos: [0.0, 0.0],
                radius: 8.0,
                label: "a".into(),
                fill: Rgb(0, 0, 0),
            }],
            edges: Vec::new(),
            palette: Palette {
                background: Rgb(255, 255, 255),
                edge: Rgb(0, 0, 0),
                node: Rgb(0, 0, 0),
                journal: Rgb(0, 0, 0),
                tag: Rgb(0, 0, 0),
                accent: Rgb(0, 0, 0),
                text: Rgb(0, 0, 0),
            },
            labels: true,
        };
        let svg = with_extension(dir.path().join("graph"), Format::Svg);
        assert_eq!(svg.extension().and_then(|e| e.to_str()), Some("svg"));
        write_export(&scene, Format::Svg, &svg).expect("svg");
        assert!(
            std::fs::read_to_string(&svg)
                .expect("read")
                .contains("<circle")
        );
        let png = with_extension(dir.path().join("pic.png"), Format::Png);
        write_export(&scene, Format::Png, &png).expect("png");
        assert_eq!(&std::fs::read(&png).expect("read")[..4], b"\x89PNG");
        // No temp file is left behind.
        assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 2);
        // An existing extension is kept.
        assert_eq!(
            with_extension(PathBuf::from("x.svg"), Format::Png),
            PathBuf::from("x.svg")
        );
    }

    #[test]
    fn flatten_blends_alpha_over_the_base() {
        let half = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 0.5,
        };
        let Rgb(r, g, b) = flatten(half, Rgb(255, 255, 255));
        assert!((126..=129).contains(&r) && r == g && g == b);
    }
}
