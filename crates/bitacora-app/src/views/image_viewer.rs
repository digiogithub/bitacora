//! Full-window image viewer: a block image opened from its hover button covers the app, the
//! mouse wheel zooms around the pointer, dragging pans, and Escape or the close button leaves.
//!
//! Block images are drawn by `RenderOnce` elements without a handle on the workspace, so the
//! workspace registers its viewer in the [`ImageViewerSlot`] global and the button opens it
//! through [`open_image`].

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use rust_i18n::t;

use crate::ui::canvas::{ScrollWheelEvent, canvas};
use crate::ui::text_edit::{
    ImgResourceLoader, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    Resource, SMOOTH_SVG_SCALE_FACTOR, img,
};
use crate::ui::{
    App, Bounds, Context, FocusHandle, Global, IconName, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Pixels, Point, Render, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, Tooltip, WeakEntity, Window, div, h_flex, icon, px,
};
use crate::views::dims;

/// Smallest and largest zoom, relative to the fitted size.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 20.0;
/// Zoom factor per 100 px of wheel travel.
const WHEEL_STEP: f32 = 1.25;
/// Share of the window the fitted image may take.
const FIT_MARGIN: f32 = 0.92;

/// The workspace's viewer, reachable from block elements.
#[derive(Debug)]
pub struct ImageViewerSlot(pub WeakEntity<ImageViewer>);

impl Global for ImageViewerSlot {}

/// Opens `path` in the registered viewer; a no-op without one.
pub fn open_image(path: PathBuf, window: &mut Window, cx: &mut App) {
    let Some(viewer) = cx
        .try_global::<ImageViewerSlot>()
        .and_then(|slot| slot.0.upgrade())
    else {
        return;
    };
    viewer.update(cx, |viewer, cx| viewer.open(path, window, cx));
}

/// The overlay state.
#[derive(Debug)]
pub struct ImageViewer {
    focus: FocusHandle,
    path: Option<PathBuf>,
    /// Multiplier of the fitted size.
    zoom: f32,
    /// Offset of the image centre from the viewport centre.
    pan: Point<Pixels>,
    /// Pointer position of an ongoing drag.
    drag: Option<Point<Pixels>>,
    /// The viewport, measured at paint time.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// What had the focus before the viewer opened.
    restore: Option<FocusHandle>,
}

impl ImageViewer {
    /// A closed viewer.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            path: None,
            zoom: 1.0,
            pan: Point::default(),
            drag: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            restore: None,
        }
    }

    /// Whether an image is shown.
    pub fn is_open(&self) -> bool {
        self.path.is_some()
    }

    /// The image shown, if any.
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// The current zoom (1 = fitted to the window).
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Shows `path` fitted to the window and takes the focus so Escape reaches the viewer.
    pub fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.path.is_none() {
            self.restore = window.focused(cx);
        }
        self.path = Some(path);
        self.zoom = 1.0;
        self.pan = Point::default();
        self.drag = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Hides the viewer and gives the focus back.
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.path.take().is_none() {
            return;
        }
        self.drag = None;
        if let Some(previous) = self.restore.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    /// Zooms by `factor` keeping the image point under `anchor` (window coordinates) still.
    pub fn zoom_by(&mut self, factor: f32, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        let next = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if (next - self.zoom).abs() < f32::EPSILON {
            return;
        }
        let ratio = next / self.zoom;
        let center = self.bounds.get().center() + self.pan;
        // anchor - center' = (anchor - center) * ratio
        self.pan += (anchor - center) * (1.0 - ratio);
        self.zoom = next;
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.zoom = 1.0;
        self.pan = Point::default();
        cx.notify();
    }
}

/// The size an image of `natural` size takes at `zoom` in a viewport of `viewport` size: fitted
/// inside the viewport margin (never upscaled), then multiplied by the zoom.
pub fn displayed_size(natural: (f32, f32), viewport: (f32, f32), zoom: f32) -> (f32, f32) {
    let (nw, nh) = (natural.0.max(1.0), natural.1.max(1.0));
    let fit = (viewport.0 * FIT_MARGIN / nw)
        .min(viewport.1 * FIT_MARGIN / nh)
        .clamp(0.0, 1.0);
    (nw * fit * zoom, nh * fit * zoom)
}

impl Render for ImageViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(path) = self.path.clone() else {
            return div().into_any_element();
        };
        let natural = window
            .use_asset::<ImgResourceLoader>(&Resource::Path(path.clone().into()), cx)
            .and_then(Result::ok)
            .map(|data| data.size(0));
        let is_svg = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
        let scale = if is_svg { SMOOTH_SVG_SCALE_FACTOR } else { 1.0 };
        let viewport = self.bounds.get();
        let image = natural.map(|n| {
            let (w, h) = displayed_size(
                (n.width.0 as f32 / scale, n.height.0 as f32 / scale),
                (
                    f32::from(viewport.size.width),
                    f32::from(viewport.size.height),
                ),
                self.zoom,
            );
            let left = f32::from(viewport.size.width) / 2.0 + f32::from(self.pan.x) - w / 2.0;
            let top = f32::from(viewport.size.height) / 2.0 + f32::from(self.pan.y) - h / 2.0;
            img(path.clone())
                .object_fit(ObjectFit::Contain)
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(w))
                .h(px(h))
        });
        let bounds = self.bounds.clone();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let percent = format!("{:.0}%", self.zoom * 100.0);

        div()
            .id("image-viewer")
            .key_context("ImageViewer")
            .track_focus(&self.focus)
            .role(crate::ui::a11y::Role::Dialog)
            .aria_label(t!("image_viewer.title").to_string())
            .absolute()
            .inset_0()
            // Occlude the app: the wheel zooms the image, never scrolls the page below.
            .occlude()
            .overflow_hidden()
            .bg(dims::VIEWER_BACKDROP)
            .cursor_grab()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => this.close(window, cx),
                    "0" => this.reset(cx),
                    "+" | "=" => {
                        let c = this.bounds.get().center();
                        this.zoom_by(WHEEL_STEP, c, cx);
                    }
                    "-" => {
                        let c = this.bounds.get().center();
                        this.zoom_by(1.0 / WHEEL_STEP, c, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, window, cx| {
                let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                if dy != 0.0 {
                    this.zoom_by(WHEEL_STEP.powf(dy / 100.0), e.position, cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    if e.click_count >= 2 {
                        this.reset(cx);
                    } else {
                        this.drag = Some(e.position);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                let Some(from) = this.drag else {
                    return;
                };
                if e.pressed_button != Some(MouseButton::Left) {
                    this.drag = None;
                    return;
                }
                this.pan += e.position - from;
                this.drag = Some(e.position);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = None),
            )
            .child(
                canvas(
                    move |b, _, _| {
                        bounds.set(b);
                        b
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(image)
            .child(
                h_flex()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .p_3()
                    .gap_3()
                    .items_center()
                    .text_sm()
                    .text_color(dims::ON_IMAGE_FG)
                    .child(div().flex_1().min_w_0().truncate().child(name))
                    .child(div().child(percent))
                    .child(
                        div()
                            .id("image-viewer-close")
                            .role(crate::ui::a11y::Role::Button)
                            .aria_label(t!("image_viewer.close").to_string())
                            .tooltip(|window, cx| {
                                Tooltip::new(t!("image_viewer.close").to_string()).build(window, cx)
                            })
                            .p_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(dims::ON_IMAGE_CHIP)
                            .hover(|s| s.bg(dims::ON_IMAGE_CHIP_HOVER))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)))
                            .child(icon(IconName::Close)),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_images_fit_the_viewport_and_small_ones_stay_natural() {
        assert_eq!(
            displayed_size((2000.0, 1000.0), (1000.0, 1000.0), 1.0),
            (920.0, 460.0)
        );
        assert_eq!(
            displayed_size((200.0, 100.0), (1000.0, 1000.0), 1.0),
            (200.0, 100.0)
        );
        assert_eq!(
            displayed_size((200.0, 100.0), (1000.0, 1000.0), 2.0),
            (400.0, 200.0)
        );
    }
}
