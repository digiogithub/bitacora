//! View tests: the gallery is rendered in a test window and driven with real mouse and keyboard
//! events.

use super::{Gallery, Glyph};
use crate::settings::AppSettings;
use crate::theme;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, Mode};
use crate::ui::{Bounds, Entity, KeyDownEvent, KeyUpEvent, Keystroke, Pixels, Window, point, px};

fn open(cx: &mut TestAppContext, mode: Mode) -> (Entity<Gallery>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
        BitacoraTheme::set_mode(mode, cx);
    });
    let (gallery, cx) = cx.add_window_view(|_: &mut Window, _| Gallery::new());
    cx.run_until_parked();
    (gallery, cx)
}

fn bounds(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(id)
        .unwrap_or_else(|| panic!("`{id}` was not rendered"))
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let at = bounds(cx, id).center();
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
}

#[gpui_test]
fn every_component_renders_in_both_modes_at_its_token_size(cx: &mut TestAppContext) {
    for mode in [Mode::Light, Mode::Dark] {
        let (_, cx) = open(cx, mode);
        let m = cx.update(|_, cx| cx.bitacora().metrics.clone());
        let heights = [
            ("gallery-primary", m.icon_button),
            ("gallery-secondary", m.icon_button),
            ("gallery-ghost", m.icon_button),
            ("gallery-disabled", m.icon_button),
            ("gallery-ai", m.icon_button - m.space[2]),
            ("gallery-icon", m.icon_button),
            ("gallery-icon-sm", m.icon_button_sm),
            ("gallery-pill-all", m.icon_button_sm),
            ("gallery-tab-0", m.tab_height),
        ];
        for (id, h) in heights {
            assert_eq!(bounds(cx, id).size.height, h, "{id} in {mode:?}");
        }
        assert_eq!(bounds(cx, "gallery-icon").size.width, m.icon_button);
        assert_eq!(bounds(cx, "gallery-icon-sm").size.width, m.icon_button_sm);
        // The popover shell is not part of the page until it is opened.
        assert!(cx.debug_bounds("gallery-popover").is_none());
        // Every glyph of the set is drawn as an icon button.
        for g in Glyph::ALL {
            let id: &'static str = Box::leak(format!("gallery-glyph-{g:?}").into_boxed_str());
            bounds(cx, id);
        }
    }
}

#[gpui_test]
fn enabled_buttons_call_their_handler_and_disabled_ones_do_not(cx: &mut TestAppContext) {
    let (gallery, cx) = open(cx, Mode::Light);
    click(cx, "gallery-primary");
    click(cx, "gallery-primary");
    click(cx, "gallery-disabled");
    gallery.read_with(cx, |g, _| {
        assert_eq!(g.primary_clicks, 2);
        assert_eq!(g.disabled_clicks, 0);
    });
}

#[gpui_test]
fn a_focused_button_activates_with_enter(cx: &mut TestAppContext) {
    let (gallery, cx) = open(cx, Mode::Light);
    // Pressing the button focuses it (one activation) ...
    click(cx, "gallery-primary");
    // The element learns it is focused when it is painted again.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // ... and Enter on the focused button activates it again, without the mouse.
    let enter = Keystroke::parse("enter").expect("keystroke");
    cx.simulate_event(KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke: enter });
    cx.run_until_parked();
    gallery.read_with(cx, |g, _| assert_eq!(g.primary_clicks, 2));
}

#[gpui_test]
fn pills_tabs_and_segments_select(cx: &mut TestAppContext) {
    let (gallery, cx) = open(cx, Mode::Dark);
    click(cx, "gallery-pill-open");
    gallery.read_with(cx, |g, _| assert!(!g.pill_all));
    click(cx, "gallery-tab-2");
    gallery.read_with(cx, |g, _| assert_eq!(g.tab, 2));
    click(cx, "gallery-segmented-grid");
    gallery.read_with(cx, |g, _| assert_eq!(g.segment.as_ref(), "grid"));
    click(cx, "gallery-segmented-list");
    gallery.read_with(cx, |g, _| assert_eq!(g.segment.as_ref(), "list"));
}

#[gpui_test]
fn popover_closes_on_escape(cx: &mut TestAppContext) {
    let (gallery, cx) = open(cx, Mode::Light);
    click(cx, "gallery-popover-toggle");
    gallery.read_with(cx, |g, _| assert!(g.popover_open));
    bounds(cx, "gallery-popover");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    gallery.read_with(cx, |g, _| assert!(!g.popover_open));
    assert!(cx.debug_bounds("gallery-popover").is_none());
}

#[gpui_test]
fn popover_closes_on_an_outside_press_but_not_an_inside_one(cx: &mut TestAppContext) {
    let (gallery, cx) = open(cx, Mode::Dark);
    click(cx, "gallery-popover-toggle");
    click(cx, "gallery-popover");
    gallery.read_with(cx, |g, _| {
        assert!(g.popover_open, "an inside press keeps it open");
    });
    let shell = bounds(cx, "gallery-popover");
    cx.simulate_click(
        point(shell.right() + px(40.0), shell.top()),
        Default::default(),
    );
    cx.run_until_parked();
    gallery.read_with(cx, |g, _| assert!(!g.popover_open));
}
