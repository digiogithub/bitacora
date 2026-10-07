//! Frameless window chrome (BIT-US-0120/0121, ADR-033): the 52px `AppTitleBar` and the
//! client-decoration window options.
//!
//! Built on the same GPUI primitives as the kit's `TitleBar` (Apache-2.0, see
//! `docs/design/frameless-window.md` section 4) but with the design-system height and
//! three content slots. The resize band, shadow ring and tiling insets come from the kit's
//! `window_border`, which the workspace wraps around the whole window.
//!
//! Behaviour is always decided from the **granted** `Window::window_decorations()`, never
//! from the requested mode:
//! - `Client` on Linux: min/max/close drawn here, filtered by `window_controls()`, right
//!   click opens the compositor window menu, double click toggles maximize.
//! - `Server`: no controls (the OS draws them); only the slots are drawn.
//! - macOS: native traffic lights, `TRAFFIC_LIGHT_INSET` reserved on the left.
//! - Windows: buttons carry `WindowControlArea` so the OS hit test gives snap layouts.

use crate::ui::frameless::{
    Decorations, InteractiveElementExt as _, MouseButton, WindowControlArea, WindowControls,
    WindowDecorations,
};
use crate::ui::theme::Palette;
use crate::ui::{
    ActiveTheme as _, AnyElement, App, Bounds, ClickEvent, FluentBuilder as _, Hsla, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    Size, StatefulInteractiveElement as _, Styled as _, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, h_flex, icon, point, px,
};
use crate::views::dims;

/// Height of the app title bar in logical pixels (design system layout).
pub const TITLE_BAR_HEIGHT: f32 = 52.0;
/// Width of one window control button.
const CONTROL_WIDTH: f32 = 46.0;
/// Left space reserved for the native macOS traffic lights.
pub const TRAFFIC_LIGHT_INSET: f32 = 78.0;
/// Left padding on platforms without traffic lights.
const LEFT_PADDING: f32 = 12.0;

/// One window control button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Minimize.
    Minimize,
    /// Maximize (window currently not maximized).
    Maximize,
    /// Restore (window currently maximized).
    Restore,
    /// Close.
    Close,
}

impl Control {
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Restore => "restore",
            Self::Close => "close",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Maximize => IconName::WindowMaximize,
            Self::Restore => IconName::WindowRestore,
            Self::Close => IconName::WindowClose,
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

/// Which controls the title bar draws. Empty on macOS (native lights) and under server
/// decorations (the OS draws its own); otherwise filtered by what the compositor honours
/// (a tiling compositor may deny minimize and maximize; close is always ours).
pub fn controls_for(
    macos: bool,
    decorations: Decorations,
    supported: WindowControls,
    maximized: bool,
) -> Vec<Control> {
    if macos || !matches!(decorations, Decorations::Client { .. }) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if supported.minimize {
        out.push(Control::Minimize);
    }
    if supported.maximize {
        out.push(if maximized {
            Control::Restore
        } else {
            Control::Maximize
        });
    }
    out.push(Control::Close);
    out
}

/// Left padding of the bar: the traffic-light inset on macOS, a small gutter elsewhere.
pub fn left_inset(macos: bool) -> f32 {
    if macos {
        TRAFFIC_LIGHT_INSET
    } else {
        LEFT_PADDING
    }
}

/// Window options for the main window: client decorations on Linux, transparent system
/// titlebar on macOS/Windows, app-owned titlebar drag, title kept for taskbar/alt-tab.
pub fn main_window_options(
    title: impl Into<SharedString>,
    bounds: Bounds<Pixels>,
    min_size: Size<Pixels>,
) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            // Centre the 14px native lights vertically in the 52px bar.
            traffic_light_position: Some(point(dims::PX_18, px((TITLE_BAR_HEIGHT - 14.0) / 2.0))),
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(min_size),
        window_decorations: Some(WindowDecorations::Client),
        app_owns_titlebar_drag: true,
        ..Default::default()
    }
}

/// Palette for the current theme mode (narrow seam: rewire to the theme global later).
fn palette(cx: &App) -> Palette {
    if cx.theme().is_dark() {
        Palette::dark()
    } else {
        Palette::light()
    }
}

/// The 52px application title bar with `left`, `center` and `right` placeholder slots
/// (filled by BIT-US-0122: tabs, search, buttons).
#[derive(IntoElement)]
pub struct AppTitleBar {
    left: Vec<AnyElement>,
    center: Vec<AnyElement>,
    right: Vec<AnyElement>,
    center_min: Pixels,
}

impl std::fmt::Debug for AppTitleBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppTitleBar")
            .field("left", &self.left.len())
            .field("center", &self.center.len())
            .field("right", &self.right.len())
            .finish()
    }
}

impl Default for AppTitleBar {
    fn default() -> Self {
        Self::new()
    }
}

impl AppTitleBar {
    /// Empty bar.
    pub fn new() -> Self {
        Self {
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
            center_min: px(0.),
        }
    }

    /// Adds content to the left slot (after the traffic-light inset).
    pub fn left(mut self, el: impl IntoElement) -> Self {
        self.left.push(el.into_any_element());
        self
    }

    /// Adds content to the flexible centre slot.
    pub fn center(mut self, el: impl IntoElement) -> Self {
        self.center.push(el.into_any_element());
        self
    }

    /// Smallest width the centre slot keeps; the left slot gives way (clips) before the
    /// centre drops below it, and the centre never paints outside its own bounds.
    pub fn center_min_width(mut self, min: Pixels) -> Self {
        self.center_min = min;
        self
    }

    /// Adds content to the right slot (before the window controls).
    pub fn right(mut self, el: impl IntoElement) -> Self {
        self.right.push(el.into_any_element());
        self
    }
}

fn close_window(window: &mut Window, cx: &mut App) {
    // Same path as the Quit menu entry: the workspace shuts the session down first.
    window.dispatch_action(Box::new(crate::actions::Quit), cx);
}

fn control_button(control: Control, pal: Palette, cx: &App) -> impl IntoElement {
    let close = control == Control::Close;
    let hover_bg: Hsla = if close { cx.theme().danger } else { pal.hover };
    let hover_fg: Hsla = if close {
        cx.theme().danger_foreground
    } else {
        pal.text
    };
    div()
        .id(control.id())
        .flex()
        .items_center()
        .justify_center()
        .w(px(CONTROL_WIDTH))
        .h_full()
        .flex_shrink_0()
        .text_color(pal.text_2)
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        .when(cfg!(target_os = "windows"), |d| {
            d.window_control_area(control.area())
        })
        .when(!cfg!(target_os = "windows"), |d| {
            d.on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                match control {
                    Control::Minimize => window.minimize_window(),
                    Control::Maximize | Control::Restore => window.zoom_window(),
                    Control::Close => close_window(window, cx),
                }
            })
        })
        .child(icon(control.icon()))
}

impl RenderOnce for AppTitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let macos = cfg!(target_os = "macos");
        let decorations = window.window_decorations();
        let client = matches!(decorations, Decorations::Client { .. });
        let pal = palette(cx);
        let controls = controls_for(
            macos,
            decorations,
            window.window_controls(),
            window.is_maximized(),
        );
        let left_pad = if window.is_fullscreen() {
            LEFT_PADDING
        } else {
            left_inset(macos)
        };

        let drag = h_flex()
            .id("app-title-drag")
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap_3()
            .pl(px(left_pad))
            .window_control_area(WindowControlArea::Drag)
            .when(!cfg!(target_os = "windows"), |d| {
                d.on_mouse_down(MouseButton::Left, |ev, window, _| {
                    // A double click is handled below; only a single press starts a move.
                    if ev.click_count == 1 {
                        window.start_window_move();
                    }
                })
            })
            .when(cfg!(target_os = "linux"), |d| {
                d.on_double_click(|_, window, _| window.zoom_window())
            })
            .when(macos, |d| {
                d.on_double_click(|_, window, _| window.titlebar_double_click())
            })
            .when(cfg!(target_os = "linux") && client, |d| {
                d.on_mouse_down(MouseButton::Right, |ev, window, _| {
                    window.show_window_menu(ev.position)
                })
            })
            .child(
                // Each slot clips its own content so one slot never paints over another
                // (the bug: a fixed-width search field spilled over tabs and buttons).
                h_flex()
                    .debug_selector(|| "title-left".to_string())
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap_3()
                    .overflow_hidden()
                    .children(self.left),
            )
            .child(
                h_flex()
                    .debug_selector(|| "title-center".to_string())
                    .flex_1()
                    .min_w(self.center_min)
                    .h_full()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .children(self.center),
            )
            .child(
                h_flex()
                    .debug_selector(|| "title-right".to_string())
                    .flex_shrink_0()
                    .h_full()
                    .items_center()
                    .children(self.right),
            );

        div().flex_shrink_0().child(
            h_flex()
                .id("app-title-bar")
                .w_full()
                .h(px(TITLE_BAR_HEIGHT))
                .items_center()
                .bg(pal.side)
                .border_b_1()
                .border_color(pal.line)
                .child(drag)
                .child(
                    h_flex()
                        .id("window-controls")
                        .debug_selector(|| "window-controls".to_string())
                        .h_full()
                        .flex_shrink_0()
                        .items_center()
                        .children(controls.into_iter().map(|c| control_button(c, pal, cx))),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::frameless::Tiling;

    fn client() -> Decorations {
        Decorations::Client {
            tiling: Tiling::default(),
        }
    }

    fn all() -> WindowControls {
        WindowControls {
            fullscreen: true,
            maximize: true,
            minimize: true,
            window_menu: true,
        }
    }

    #[test]
    fn client_draws_all_supported_controls() {
        assert_eq!(
            controls_for(false, client(), all(), false),
            [Control::Minimize, Control::Maximize, Control::Close]
        );
        assert_eq!(
            controls_for(false, client(), all(), true),
            [Control::Minimize, Control::Restore, Control::Close]
        );
    }

    #[test]
    fn tiling_compositor_keeps_only_close() {
        let none = WindowControls {
            minimize: false,
            maximize: false,
            ..all()
        };
        assert_eq!(controls_for(false, client(), none, false), [Control::Close]);
    }

    #[test]
    fn server_decorations_and_macos_draw_no_controls() {
        assert!(controls_for(false, Decorations::Server, all(), false).is_empty());
        assert!(controls_for(true, client(), all(), false).is_empty());
    }

    #[test]
    fn macos_reserves_the_traffic_light_inset() {
        assert_eq!(left_inset(true), TRAFFIC_LIGHT_INSET);
        assert!(left_inset(false) < TRAFFIC_LIGHT_INSET);
    }

    #[test]
    fn main_window_options_are_frameless_but_titled() {
        let o = main_window_options(
            "Bitacora",
            Bounds::default(),
            crate::ui::size(px(640.), px(400.)),
        );
        assert_eq!(o.window_decorations, Some(WindowDecorations::Client));
        let t = o.titlebar.expect("titlebar");
        assert!(t.appears_transparent);
        assert_eq!(t.title.as_deref(), Some("Bitacora"));
        assert!(o.app_owns_titlebar_drag);
        assert!(o.window_min_size.is_some());
    }

    struct Host;

    impl crate::ui::Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut crate::ui::Context<Self>) -> impl IntoElement {
            AppTitleBar::new()
                .left(div().w(px(10.)))
                .center(div().w(px(10.)))
                .right(div().w(px(10.)))
        }
    }

    #[crate::ui::testing::gpui_test]
    fn title_bar_renders_with_all_slots(cx: &mut crate::ui::testing::TestAppContext) {
        cx.update(crate::ui::init);
        let (_host, cx) = cx.add_window_view(|_, _| Host);
        cx.run_until_parked();
    }
}
