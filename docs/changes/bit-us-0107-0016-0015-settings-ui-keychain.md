---
created_at: 2026-10-06T21:54:13.734743865Z
updated_at: 2026-10-06T21:54:13.734743865Z
tags:
    - change
    - bitacora-app
    - settings
    - mcp
---
# Settings UI, keychain token storage (BIT-US-0107, BIT-US-0016 remainder, BIT-US-0015 remainder)

Links: [[bitacora-full-development-plan]], [[mcp-server]], [[changes/bit-us-0043-0046-0047-0048-0054-0070-sync-ui.md]], [[changes/bit-us-0015-0016-mcp-server-skeleton-auth.md]], [[changes/runtime-followups-config-reload.md]].

## What changed
- `crates/bitacora-app/src/views/settings/`: `mod.rs` (SettingsView, Section, SettingsEvent, Pending confirmations), `graph_config.rs` (GraphEdit, ApplyMode, edit_config via ConfigEditor + command queue, date format validation), `sections.rs` (General, Editor, Search, Sync, Appearance), `agents.rs` (tokens, toggles, snippets), `keymap.rs` + `keymap_model.rs` (editor, conflicts, user keymap.json with `unbind`), `model.rs` (AppKey apply modes, parsers), `tests.rs`.
- Hooks: workspace.rs (settings entity, open_settings, on_settings_event, SessionOptions from app settings), palette `OpenSettings`, action + `secondary-,`, app menu (`ui::set_app_menu`), `settings.en.yml`, `keymap.rs` (`Section.unbind`, `effective/diff/apply_live`), `theme::edit_settings/try_settings`, font size.
- `settings.rs`: `AppSettings.{font_size, search, mcp}`; session applies `search.substring` on open and `mcp.*` via `McpSettings::to_config`.
- Runtime: `Session::{mcp_tokens, set_substring}`, `McpOptions.secrets`.
- MCP: `tokens.rs` SecretBackend/KeyringBackend/MemoryBackend, file v2 (metadata only for keychain tokens), migration, fallback, summaries, set_scopes; CLI uses the keychain only for the default token path.
- Tray: no GPUI tray API; documented, closed with keep_running_in_background.

## Verification
fmt, clippy workspace -D warnings, typos, check-deps, machete, deny clean. `cargo test --workspace --locked --no-fail-fast`: 1410 passed, 0 failed. New: 20 view tests (real MCP server revoke 401, live keymap rebind driving the workspace), 9 token tests. Xvfb+lavapipe+XTest screenshots: every section, confirm dialogs, token create/revoke against curl, port change and restart, keymap record, substring persisted.

## Left
macOS/Windows keychain validation (T-0115 in_review).