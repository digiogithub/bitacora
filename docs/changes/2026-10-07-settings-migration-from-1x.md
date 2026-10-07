---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - migration
---
# Settings migration from 1.x (BIT-US-0162)

Part of [[bitacora-v2-plan]]. Tasks: BIT-T-0486 (settings and layout part; the 1.0 index DB migration test is not covered here).

## What changed
- New `crates/bitacora-app/src/migrate.rs` (`run(&AppDirs) -> Report`, `Notice`), called once at startup by the lock-owning instance in `app::run` (not for smoke/spike runs). The module doc holds the inventory of 1.x machine-local files.
- `settings.rs`: `SETTINGS_VERSION = 2` and `AppSettings.settings_version`. 1.x files have none (read as 1).
- Settings: backed up to `settings.json.1x.bak` (never overwritten), `light_theme`/`dark_theme` dropped, version stamped; all other keys kept. A non-default 1.x theme (not Paper/Midnight/null) yields a notice. Unparseable file: kept, backed up, notice.
- `workspace.json` with `version < LAYOUT_VERSION` (2): backed up and removed so the default layout is built; newer versions untouched.
- `config/themes/*.json` (no longer loaded): left in place, one-time notice (marker `.2.0-notice-shown`); colour-scheme files return after 2.0 (BIT-US-0164).
- `recent-graphs.json`, `keymap.json`, `mcp-tokens.json`: same formats, untouched (tested). `logseq/custom.css` and the graph folder are never touched.
- Notices shown as toasts (`migrate.*` keys in en/es locales).

## Verification
`cargo test -p bitacora-app --locked` 532 passed; clippy `-D warnings` clean. Fixtures: `crates/bitacora-app/tests/fixtures/settings-1.x.json`, `workspace-1.x.json`.
