---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - ui
    - title-bar
---
# Top bar content, page tabs and in-window Graph menu (BIT-US-0122)

Continues [[bitacora-v2-plan]], the frameless `AppTitleBar` (BIT-US-0120/0121) and the component kit (BIT-US-0117). Implements BIT-T-0395 and BIT-T-0396 and delivers the in-window Graph menu that BIT-T-0491 (BIT-US-0165) waited for.

## What changed
- New `crates/bitacora-app/src/views/workspace/top_bar.rs` (child module of `workspace.rs`): `impl Workspace { title_bar() }` fills the `AppTitleBar` slots.
  - left: in-window "Graph" menu button (non-macOS only, `has_in_window_menu`), sidebar toggle, back, forward, page tabs (kit `Tab`, bottom aligned) and a `+` new-tab button.
  - centre: search field (150-300px, `bg`, `line` border, `Kbd` hint) that calls `open_search`, the same handler as the `OpenSearch` shortcut.
  - right: theme (sun/moon), PDF (disabled: no export backend yet), assistant sparkle (opens Settings until the Pando epic wires it), right panel toggle.
  - every control swallows the left press so it never starts a window drag.
- `TabStrip` (pure model): one tab per open page, the active one follows `MainEvent::Visited`, tabs can be added, activated (navigates) and closed (the last tab stays).
- Graph menu popover (kit `PopoverShell` in `deferred(anchored())`): Open graph..., Open recent (the recents list, or a disabled "No recent graphs yet"), Close graph; same handlers as `menus.rs` actions.
- `workspace.rs`: `tabs`, `app_menu_open` fields, `tabs_visit` call in the `Visited` handler, `title_bar()` in `render`.
- `ui/mod.rs` re-exports `anchored`. Locale keys `top_bar.search`, `top_bar.block_tab` (en, es).

## Limits
- Tabs are an in-memory strip over the single page host; the dock layout is unchanged and its persistence is untouched. Tab reorder by drag and persisting the tabs are not implemented (the kit `Tab` has no drag source yet).
- The dock's own title strip for the page host is still drawn by the dock skin.

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-app --locked`: 445 passed (new: `TabStrip` unit tests, `#[gpui_test]` for sidebar/right panel/theme buttons, search field opening the palette, tab add/follow, Graph menu open/close).
