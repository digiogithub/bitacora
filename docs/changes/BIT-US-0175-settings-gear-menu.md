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
