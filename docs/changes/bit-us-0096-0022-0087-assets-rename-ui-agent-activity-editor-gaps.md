---
created_at: 2026-10-06T21:54:19.309361545Z
updated_at: 2026-10-06T21:54:19.309361545Z
tags:
    - change
    - bitacora-app
    - bitacora-core
    - editor
---
# BIT-US-0096, BIT-T-0157, BIT-T-0211 (US-0022) and editor gaps

Continues [[changes/bit-us-0030-0039-block-editor-ui.md]], [[changes/bit-us-0020-0023-mcp-write-tools-audit-compat-api.md]], [[changes/bit-us-0061-0082-0087-page-rename-cascade-merge.md]], [[changes/bit-us-0028-0057-0089-0099-page-lifecycle.md]]; plan [[bitacora-full-development-plan]].

## What changed
- core: `Request::OpenPage`/`Response::Opened` (virtual page, no file until content). `assets.rs` (Logseq naming `<stem>_<ms>_<index><ext>`, relative links, `is_local_asset_link`), `Cmd::ImportAssets`/`NewAsset`, `Op::ImportAsset`/`DropAsset`, `Workspace::pending_creates` written atomically at flush (undo = cancel pending create or recycle). Tests: `tests/assets_import.rs`, `tests/command_queue.rs`.
- app editor: placeholder pages editable (`editor::ensure_loaded` uses OpenPage), mouse-drag block selection (`drag_over`), Up/Down across journal days (`EditorEvent::Leave`, `enter_edge`), floating completion popup (`deferred`), debounced edit submitted without blocking the UI thread, journal days handed to core in the background. Paste of clipboard image/files and drop on rows (`editor/assets.rs`, `attach_files`), per-block delete-asset action + row button (`EditorEvent::DeleteAsset` -> `PageEvent`/`MainEvent` -> `Workspace::request_delete_asset`), `resolve_asset` falls back to `<root>/assets` and leaves `@alias` unresolved.
- rename UI: double-click page title -> input; `PageEvent::RenamePage`; `Workspace::rename_page`/`confirm_merge` (dialog lists block counts and dropped aliases), `graph_ops::rename_page`. `SessionLink` gains `gate` and `lookup`.
- MCP gate: `editing::EditingGate` (WriteGate) published by editors, wired into `McpConfig::gate`; agents get `BLOCK_BUSY` while the block has the caret.
- `views/agent_activity.rs`: overlay (palette "Agent activity..."), filters by token/tool/writes/errors, detail with jump-to-block, undo via `Session::undo_agent_entry`.
- Fixes found visually: click on an empty block never started editing; rename now exits block edit mode. Editor keystroke tests use platform modifier helpers (`k`, `kw`, `km`, `kz`) for macOS.

## Verification
fmt, clippy -D warnings, typos, check-deps, machete clean; `cargo test --workspace --locked --no-fail-fast`: 1402 passed, 0 failed. Visual (Xvfb + lavapipe + XTest, screenshots viewed): journal Down across days, drag selection, floating popup, placeholder page typing writes `pages/Ghost.md`, rename -> merge dialog -> cancel (no change) -> merge (recycle, refs rewritten), delete-asset dialog recycles the file, Agent activity list. Not verified visually: real X11 file drop / image paste (no clipboard tool on host; covered by gpui tests via `drop_files` and clipboard image).

## Limits
Queue calls other than the debounced edit still block the UI thread (they are in-memory and fast). The "edited by agent" toast linking to the view was not added. Undo of a real agent write is covered by runtime e2e tests, not by a UI test. macOS tests not run locally.
