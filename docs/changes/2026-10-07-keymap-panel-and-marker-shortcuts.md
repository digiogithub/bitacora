---
created_at: 2026-10-07T19:30:00Z
updated_at: 2026-10-07T19:30:00Z
tags:
    - change
    - app
    - keymap
---
# Keymap panel grouping and Mod+1/2/3 marker shortcuts

Story BIT-US-0170 (tasks BIT-T-0507, BIT-T-0508), part of [[bitacora-v2-plan]].

The Settings > Keymap editor already existed (BIT-T-0332: filter, record, conflict confirm, reset one/all, `keymap.json` persistence, live apply). Changes:

- `views/settings/keymap.rs` `render_keymap`: rows grouped under context headers.
- New actions `outliner::SetMarkerTodo/Doing/Done` (`editor/actions.rs`), handlers `set_marker_to` + `on_set_marker_*` in `editor/view.rs` running `Cmd::SetMarker` through `structural()` (one undoable transaction, editing block or selection).
- `assets/keymaps/default.json`: `secondary-1/2/3` in `BlockSelection` and `BlockEditor` (Ctrl on Linux/Windows, Cmd on macOS); no existing binding used them.
- Tests: `editor::tests::ctrl_1_2_3_*` (bytes + undo), `settings::tests::recording_a_keystroke_binds_it_and_a_restart_reloads_the_override`.

Verification: `cargo test -p bitacora-app --lib --locked` 613 passed (includes `every_action_is_reachable_from_the_keyboard`); clippy -D warnings clean. macOS not verified.
