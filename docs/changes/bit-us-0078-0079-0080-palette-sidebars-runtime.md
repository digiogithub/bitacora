---
created_at: 2026-10-06T19:53:40.381083863Z
updated_at: 2026-10-06T19:53:40.381083863Z
tags:
    - change
    - bitacora-app
    - ui
---
# BIT-US-0078 / 0079 / 0080 + app runtime adoption, BIT-T-0084, BIT-T-0174

Part of [[bitacora-full-development-plan]]; builds on [[changes/bit-us-0067-runtime-crate-wiring.md]], [[changes/bit-us-0075-0076-0077-page-view-journals-references.md]], [[changes/bit-us-0073-0074-graph-picker-session-block-rendering.md]], [[changes/bit-us-0009-full-text-search.md]], [[changes/bit-us-0014-0024-0025-app-shell.md]] and [[changes/bit-us-0028-0057-0089-0099-page-lifecycle.md]]. Design: [[gpui-and-gpui-kit]], [[sqlite-index-schema]]. Commits 69731c8, 0e395e1 (+ merge 547f0bb).

## What changed
**Runtime adoption (`crates/bitacora-app/src/session.rs`)**: `GraphSession` now owns a `bitacora_runtime::Session` on one thread (index + command queue + watcher + MCP with the default token file, sync off). Events: `Reader`, `Live(SessionLink{queue, config, mcp_endpoint})`, `Index(IndexEvent)` (from `Session::index_events`), `Notice(SessionNotice)` (write failed, conflict, watcher degraded, config changed, index error, MCP unavailable; MCP bind failure retries without MCP), `Ready`, `Failed`. `shutdown_with_report(budget)` = ordered shutdown; Quit action and `on_app_quit` use it, an unclean `ShutdownReport` is shown as a notice (`shutdown_problem`). The startup reconcile is internal to `Session::open`, so the old per-file progress events are gone (status shows busy until ready).
**Core/index/runtime additions**: `Request::EnsureToday` / `Response::Journal` (queue.rs); `IndexReader::{search, backlink_counts, blocks_mentioning}`; `Session::{read_api, index_events}`.
**US-0078** `views/palette.rs`: GPUI Kit `Command` overlay. Search mode (30 ms debounce, stale queries cancelled by replacing the task, pages/blocks/create-page groups, highlighted snippets, scopes All/This page/Journals/Pages, recent pages when empty) and actions mode (journals, all pages, back/forward, sidebars, theme, reindex, delete page, switch graph). Keys: Mod+K, Mod+Shift+P, Enter, Shift+Enter (right sidebar), Tab (scope), Esc (clear, then close); `g j`, `g a`, `t l`, `t r` chords (`Workspace && !Input && !Textarea`).
**US-0079**: `views/sidebar.rs` (favorites from `:favorites`, recent, current-page highlight, Shift+click), `views/all_pages.rs` (`DataTable`, sort, filter, journals toggle), `graph_state.rs` (per-graph `state.json` in the data dir: recent 20 + right sidebar stack).
**US-0080**: `views/right_sidebar.rs` (stack of cards with own `PageView`), `OpenIn`/`PageEvent::OpenInSidebar`, Shift+click on refs/tags/breadcrumbs/bullets/rows; `views/panels.rs` `PaneHub` + `SharedHub` replace `SharedMainView`: each `PageHost` panel owns a `MainView` (`MainEvent::{Visited, OpenInSidebar}`).
**T-0084**: `Workspace::start_day_clock` / `on_day_tick` (`DayRollover`) call `graph_ops::ensure_today` on open and at midnight; journals view already shows today virtually.
**T-0174**: `graph_ops.rs` (`delete_page`, `delete_asset`, `favorites_remove` via `ConfigEditor` + `QueueLock::apply`), `Workspace::request_delete_page/asset` with `ui::confirm`, palette command "Delete current page...".

## Verification
`cargo fmt`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `typos`, `cargo xtask check-deps`, `cargo machete` clean. `cargo test -p bitacora-app -p bitacora-core -p bitacora-index -p bitacora-runtime --locked`: app 166 lib tests pass (new: session 5, graph_state 3, graph_ops 4, all_pages 3, right_sidebar 3, palette 7, sidebar 2, workspace 6 new), core/index/runtime all pass (one flaky pre-existing `bitacora-index/tests/property.rs::incremental_equals_rebuild_on_a_small_graph` failed once under load and passed on rerun). Visual check under Xvfb + lavapipe with keys sent through XTest on a tempdir copy of `logseq-docs`: palette, actions palette, Shift+Enter and Shift+click to the sidebar, all pages table (sort, filter), per-graph persistence after restart, clean Ctrl+Q exit.

## Open points
- Block-level trigger for asset deletion needs the block editor (`request_delete_asset` is the entry point).
- Dock width persists globally (layout file), the stack per graph.
- `TestGraph` (testing.rs) still builds its own indexer; app tests that need the queue use `bitacora_runtime::Session` directly (graph_ops tests, workspace tests).
- Screen-reader labels and IME not verifiable on this host.
