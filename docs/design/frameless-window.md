---
created_at: 2026-10-07T10:30:00Z
updated_at: 2026-10-07T10:30:00Z
tags:
    - design
    - ui
    - bitacora-app
    - frameless
---
# Frameless window (client-side decorations) spike

> Status: **Linux verified, macOS and Windows unverified (in_review).** Written 2026-10-07 for BIT-US-0119 /
> BIT-T-0389; implements the investigation part of BIT-SP-0008.R4 and R5. Plan: [[bitacora-v2-plan]].
> Pins: `gpui-pre 0.3.8` (`gpui_kit::gpui`), `gpui-component 0.7.1` (`gpui_kit::component`).
> Proof: `crates/bitacora-app/src/spike/csd.rs`, run with `bitacora --spike-csd` (add `--smoke-test` to log the
> granted mode and exit). The production `AppTitleBar` is BIT-US-0120; this doc does not design it.

## 1. Verified API surface (gpui-pre 0.3.8)

| API | Location | Notes |
|---|---|---|
| `WindowOptions::window_decorations: Option<WindowDecorations>` | `platform.rs:2500` | `Server` (default) or `Client`. X11/Wayland only; "the platform may ignore requests it cannot satisfy". |
| `TitlebarOptions { title, appears_transparent, traffic_light_position }` | `platform.rs:2639` | `appears_transparent` hides the system titlebar on **macOS and Windows**; on Linux use `window_decorations`. |
| `WindowOptions::app_owns_titlebar_drag: bool` | `platform.rs` (`WindowOptions`) | macOS only: the app drags the window itself via `start_window_move`; AppKit then neither drags nor delays titlebar clicks. Keep `true` for a custom titlebar. |
| `Window::window_decorations() -> Decorations` | `window.rs:2904` | `Server` or `Client { tiling: Tiling }`. This is the **granted** mode, never assume the requested one. |
| `Window::window_controls() -> WindowControls` | `window.rs:2919` | `{ minimize, maximize, fullscreen, window_menu }`; the compositor says what it honours (tiling WMs may deny min/max). |
| `Window::start_window_move()` | `window.rs:2888` | Wayland (`xdg_toplevel.move`), X11 (`_NET_WM_MOVERESIZE`), macOS (`performWindowDragWithEvent`). Not used on Windows. |
| `Window::start_window_resize(ResizeEdge)` | `window.rs:2446` | Linux client decorations; no-op when the window is not resizable. |
| `Window::show_window_menu(Point<Pixels>)` | `window.rs:2880` | Linux: the compositor's titlebar context menu (right click on the bar). |
| `Window::set_client_inset(Pixels)` | `window.rs:2893` | Width of the invisible shadow ring the app paints around the frame (Linux CSD). Read back with `client_inset()`. |
| `Window::request_decorations(WindowDecorations)` | `window.rs:2423` | Re-request at runtime. GPUI calls it once at window creation with the option value. |
| `Window::zoom_window()`, `minimize_window()`, `remove_window()`, `titlebar_double_click()` | `window.rs` | Button and double-click actions (`titlebar_double_click` honours the macOS system setting). |
| `div().window_control_area(WindowControlArea::{Drag,Min,Max,Close})` | `elements/div.rs:749` | **Windows only** in effect: maps to `HTCAPTION/HTMINBUTTON/HTMAXBUTTON/HTCLOSE` hit tests (`gpui-pre-windows events.rs:976`), which gives native snap layouts and caption drag. |
| `Window::set_traffic_light_position(Point<Pixels>)` | `window.rs:2931` (macOS) | Moves the native traffic lights at runtime; initial value from `TitlebarOptions`. |

All of the above are reachable through `gpui_kit::gpui` and the needed types are exposed in the
`ui::frameless` facade (`TitleBar`, `window_border`, `Decorations`, `ResizeEdge`, `Tiling`, `WindowControls`,
`WindowDecorations`).

## 2. Per-OS behaviour

| OS / session | Result | Evidence |
|---|---|---|
| Linux Wayland, compositor with xdg-decoration (COSMIC, KDE, wlroots) | `Client` granted; `request_decorations` sets the xdg-decoration mode to client side. | **Verified** on COSMIC Wayland: `decorations: Client`, `controls: min/max/fullscreen/menu = true`, `inset: 20px`. |
| Linux Wayland, GNOME (mutter has no xdg-decoration) | GPUI always ends in `Client` (`wayland/window.rs:2135-2140`: with no decoration manager `decorations = Client`, even if `Server` was requested), so the app **must** draw its own controls. | Source reading only; no GNOME session here. |
| Wayland, compositor answers `Server` | `handle_toplevel_decoration_event` flips the state back to `Server`; `window_decorations()` reports it. | Source reading. |
| Linux X11 with a compositor and `_GTK_FRAME_EXTENTS` support | `Client` granted. | **Verified** through XWayland (`WAYLAND_DISPLAY` unset) on COSMIC: `Client`. |
| Linux X11 without a compositor | GPUI forces `Server` (`x11/window.rs:1914-1923`; `client_side_decorations_supported = compositor_present && gtk_frame_extents_supported`, `x11/client.rs:422`). | Source reading; not reproducible here. |
| macOS | `appears_transparent: true` sets `NSFullSizeContentViewWindowMask`; native traffic lights remain, positioned by `traffic_light_position`. Move through `start_window_move`; double-click through `titlebar_double_click`. | **Unverified** (no macOS host). |
| Windows | `appears_transparent` hides the system titlebar (`gpui-pre-windows window.rs:480`); the app draws min/max/close tagged with `window_control_area`, so snap layouts and caption drag come from the OS hit test. | **Unverified** (no Windows host). |

The run was headless, so only the granted mode and controls were observed; interactive move, resize,
double-click and snap were not exercised. Checklist for the maintainer on each OS: drag the bar; double-click to
maximize/restore; resize from all 8 edges/corners; tile with the OS shortcut and confirm the shadow ring
disappears; right-click the bar (Linux) for the window menu; macOS: traffic lights visible and aligned;
Windows: hover the maximize button for snap layouts.

## 3. Fallbacks

1. **Always branch on the granted mode.** `matches!(window.window_decorations(), Decorations::Client { .. })`
   decides whether to draw min/max/close, the border/shadow and the right-click menu overlay. Under `Server`
   our titlebar contributes only content (title, menu buttons), otherwise the user sees two close buttons.
   gpui-component's `TitleBar` already does this (`title_bar.rs`, `WindowControls::render`).
2. **Server decorations (X11 without compositor, or compositor refuses):** keep the layout, skip controls,
   shadow and resize zones (`window_border` returns its children untouched when not `Client`).
3. **GNOME Wayland:** client decorations are the only mode; controls come from our titlebar, filtered by
   `window_controls()`.
4. **macOS traffic lights:** never draw our own buttons. Reserve left padding for the native lights (kit: 80px)
   and set `traffic_light_position` to centre them vertically in our bar height.
5. **Windows:** do not rely on `start_window_move`; put `WindowControlArea::Drag` on the drag region so the OS
   handles dragging, snapping and the maximize hover flyout.
6. **Escape hatch:** `window_decorations: Some(Server)` (or omitting it) restores the system frame with no other
   change. An app setting for it is a decision for BIT-US-0120.

## 4. Decision: kit `TitleBar` + `window_border` vs custom `AppTitleBar`

**Recommendation: build `AppTitleBar` on top of the kit pieces, do not replace them.**

- Use `TitleBar::window_options()` as the base of `WindowOptions` (sets `appears_transparent`,
  `traffic_light_position (9, 9)` and `app_owns_titlebar_drag`), then add `window_decorations: Some(Client)`
  (the kit does not set it). The spike's `frameless_options()` does exactly this.
- Wrap the root view in `window_border()`: it paints the 20px Linux shadow ring, calls `set_client_inset`,
  starts `start_window_resize` from a 4px band, shows resize cursors and drops all of it when tiled or `Server`.
- `TitleBar` already implements drag via `start_window_move`, Linux double-click `zoom_window`, macOS
  `titlebar_double_click`, right-click `show_window_menu` (client only), Windows `window_control_area` buttons,
  Linux button handlers, control filtering from `window_controls()` and an `on_close_window` hook. Its look is
  theme-driven (`title_bar`, `title_bar_border`, default gradient) and overridable with `refine_style`.
- Gaps for US-0120: fixed `TITLE_BAR_HEIGHT = 34px` (control buttons are `w(TITLE_BAR_HEIGHT)`), children-only
  slot, square window border (GPUI cannot round content masks yet), `on_close_window` is Linux only. If these
  block the design system, wrap `TitleBar` in an own element or port its logic (gpui-component is Apache-2.0)
  instead of depending on its private constants.

## 5. Risks and notes

- `SHADOW_SIZE` is `pub(crate)` in gpui-component (20px on Linux, 0 elsewhere) and `set_client_inset` must stay
  consistent with it, so keep `window_border` as the only caller.
- Linking `bitacora-app` on a dev machine needs the `libxkbcommon-x11.so` dev symlink (`libxkbcommon-x11-dev`).
- GPUI churn: all frameless symbols sit in `ui::frameless`, so a kit bump only touches `src/ui/mod.rs`.

## Requirements

- MUST branch on `Window::window_decorations()` rather than on the requested option.
- MUST NOT draw window controls on macOS or under `Server` decorations.
- SHOULD reuse kit `TitleBar` and `window_border` (section 4) unless US-0120 proves a blocker.

## Open questions

- Interactive move/resize/snap results on GNOME Wayland, KDE, macOS and Windows (maintainer, in_review).
- Should the user be able to force system decorations from settings?
