---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - changes
    - bitacora-app
    - v2
---
# 2026-10-07 — Reopen the last graph at startup and graph menu (BIT-US-0165)

Part of [[bitacora-v2-plan]]. Story BIT-US-0165, tasks BIT-T-0490 and BIT-T-0491.

## What changed
- Startup without `--graph` now reopens the most recent graph from `recent-graphs.json` (the machine-local user profile). If its folder is gone, a warning is shown, the entry is pruned and the picker stays visible. New setting `reopen_last_graph` (default on, Settings > General) turns it off.
- Graph menu: "Open graph...", "Open recent" submenu and "Close graph" (native menu bar on macOS, rebuilt whenever the recents change), plus palette commands `OpenGraph` / `CloseGraph` and keys `secondary-o` / `secondary-shift-w`. `SwitchGraph` and the sidebar switcher still show the picker.
- `--graph` and second-instance launches (BIT-T-0153) are unchanged.

## Files and symbols
- `crates/bitacora-app/src/views/workspace.rs`: `Workspace::open_startup_graph`, `open_graph_dialog`, `open_recent`, `close_graph`, `persist_recents`; action handlers; 4 new `#[gpui_test]`s.
- `crates/bitacora-app/src/app.rs`: calls `open_startup_graph` when no `--graph`; installs menus via `menus::install`.
- `crates/bitacora-app/src/menus.rs` (new): `build`, `install`, `recent_action`.
- `crates/bitacora-app/src/ui/mod.rs`: `MenuEntry`, `MenuSpec`, `set_menus` replace `set_app_menu`.
- `src/actions.rs`, `src/settings.rs` (`AppSettings::reopen_last_graph`, manual `Default`), `src/views/palette.rs`, `src/views/settings/sections.rs`, `src/keymap.rs`, `assets/keymaps/default.json`, `assets/locales/*.yml`.

## Open points
- Linux/Windows have no native menu bar; an in-window menu on the frameless top bar is not built yet (palette, keys and the sidebar switcher cover those platforms). macOS menu needs manual verification.

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-app --locked`: 405 lib tests pass (3 runs). Tests: `startup_reopens_the_last_graph_without_the_picker`, `startup_with_a_missing_last_graph_shows_the_picker_and_prunes`, `startup_without_recents_or_with_the_setting_off_shows_the_picker`, `graph_menu_actions_open_recent_and_close`, `menus::tests`.
