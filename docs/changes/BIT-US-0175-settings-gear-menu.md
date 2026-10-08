# BIT-US-0175: settings gear menu in the title bar

Part of [[2-0-x-owner-feedback-plan]].

## What changed
- `crates/bitacora-app/src/menus.rs`: new pure model `gear_rows(recents)` (`GearRow`, `GearCommand`): "Settings...", one row per `Section::ALL`, then open graph / recent / close graph and Quit (same i18n keys as `build`).
- `crates/bitacora-app/src/views/workspace/top_bar.rs`: `Workspace::gear_menu` / `set_gear_menu` render a gear button (tooltip "Settings (Ctrl+,)", Cmd on macOS) at the right end of the title bar actions, with a `PopoverShell` listing the model rows. Rows call `open_settings(Some(section))`, `open_graph_dialog`, `open_recent`, `close_graph`, `cx.quit()`. The "Graph" popover is kept.
- `views/workspace.rs`: `gear_menu_open` state; UI test `gear_menu_opens_and_lists_settings_sections`.
- Locales: `top_bar.settings_tip`, `top_bar.settings_tip_mac` (en, es).
- Keymap: `secondary-,` -> `bitacora::OpenSettings` already existed in `assets/keymaps/default.json`; a test now pins it.

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` passes. Tests could not be linked on this host (missing `libxkbcommon-x11`).

## Follow-up 2026-10-08 (owner validation)
- The gear no longer opens a list: `top_bar.rs::gear_menu` is a plain button (tooltip unchanged) whose click calls `open_settings(None)`. Removed the popover, `menus::{gear_rows, GearRow, GearCommand}`, `Workspace::gear_menu_open` / `set_gear_menu` and their test. The "Graph" popover stays.
- Test `gear_click_opens_the_settings_screen` replaces `gear_menu_opens_and_lists_settings_sections`.
- Wheel capture: `views/modal.rs::modal` overlay now calls `.occlude()` so wheel events never reach the views behind a modal; the settings section list (`settings-nav`) is now an `overflow_y_scroll` container like the section body (`settings-content`, already scrollable).
- Verification: `cargo test -p bitacora-app --locked` (640 passed), `cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean. No automated test of the wheel occlusion (needs a scrollable pane behind to observe; GPUI test harness cannot assert hit-test blocking cheaply). Not verified visually.
