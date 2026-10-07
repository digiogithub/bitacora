//! Client-side decorations spike (BIT-US-0119). Throwaway: it validates the pinned
//! GPUI API (`WindowDecorations::Client`, transparent titlebar, kit `TitleBar` and
//! `window_border`) and reports what the platform actually granted. The production
//! `AppTitleBar` is BIT-US-0120. Findings: `docs/design/frameless-window.md`.

use anyhow::Context as _;

use crate::ui::{
    self, ActiveTheme as _, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, Window, WindowOptions, div, frameless, px, size,
};

/// Window options for a frameless (client-decorated) window.
///
/// - Linux (X11/Wayland): `window_decorations: Client`. Wayland compositors without
///   xdg-decoration (GNOME) are always client side; X11 without a compositor falls back
///   to `Server` inside GPUI, so callers must branch on `Window::window_decorations()`.
/// - macOS/Windows: `appears_transparent` hides the system titlebar; the traffic lights
///   (macOS) stay native at `traffic_light_position`.
pub fn frameless_options() -> WindowOptions {
    WindowOptions {
        window_decorations: Some(frameless::WindowDecorations::Client),
        window_min_size: Some(size(px(480.), px(320.))),
        ..frameless::TitleBar::window_options()
    }
}

/// Human-readable summary of what the platform granted.
pub fn describe(window: &Window) -> String {
    let mode = match window.window_decorations() {
        frameless::Decorations::Server => "Server".to_owned(),
        frameless::Decorations::Client { tiling } => format!("Client (tiling {tiling:?})"),
    };
    let c = window.window_controls();
    format!(
        "decorations: {mode}\ncontrols: min={} max={} fullscreen={} menu={}\ninset: {:?}",
        c.minimize,
        c.maximize,
        c.fullscreen,
        c.window_menu,
        window.client_inset()
    )
}

struct CsdSpike;

impl Render for CsdSpike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let info = describe(window);
        frameless::window_border().child(
            ui::v_flex()
                .size_full()
                .bg(cx.theme().background)
                .child(frameless::TitleBar::new().child("Bitacora CSD spike"))
                .child(div().p_4().child(info)),
        )
    }
}

/// Opens the spike window; with `smoke` it logs the granted mode and quits.
pub fn open(cx: &mut App, smoke: bool) -> anyhow::Result<()> {
    let (handle, _view) =
        ui::open_main_window(frameless_options(), cx, |_, cx| cx.new(|_| CsdSpike))
            .context("opening the CSD spike window")?;
    if smoke {
        handle.update(cx, |_, window, _| {
            window.on_next_frame(|window, cx| {
                tracing::info!("csd spike: {}", describe(window).replace('\n', " | "));
                cx.quit();
            });
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_request_client_decorations_and_app_owned_drag() {
        let o = frameless_options();
        assert_eq!(
            o.window_decorations,
            Some(frameless::WindowDecorations::Client)
        );
        let t = o.titlebar.expect("titlebar options");
        assert!(t.appears_transparent);
        assert!(t.traffic_light_position.is_some());
        assert!(o.app_owns_titlebar_drag);
    }
}
