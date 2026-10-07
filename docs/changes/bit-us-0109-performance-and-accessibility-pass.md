---
created_at: 2026-10-07T00:10:49.373082431Z
updated_at: 2026-10-07T00:10:49.373082431Z
tags:
    - change
    - performance
    - accessibility
    - app
    - index
---
# BIT-US-0109 Performance and accessibility pass for 1.0 (T-0336, T-0337, T-0338)

Continues [[bitacora-full-development-plan]]; reports [[performance-report-1.0]] and [[accessibility-1.0]]; builds on [[bit-us-0040-0060-0072-block-editor-spike]], [[bit-us-0009-full-text-search]], [[bit-us-0108-themes-custom-css-translations]]; decisions in ADR-026 ([[architecture]]), [[sqlite-index-schema]] section 3.1, [[gpui-and-gpui-kit]] section 1.10.

## What changed
- **Instrumentation** (`crates/bitacora-app/src/perf.rs`, `perf/bench.rs`, `perf/frame.rs`): opt-in marks/spans/stamps (`BITACORA_PERF=1`), `--perf-bench` driver for the real workspace printing `PERF_BENCH {json}`, `FrameEnd` marker element for CPU-side frame time. Hooks: `Workspace::graph_handle`, `PageView::list_state`, `JournalsView::list_state`, spans in `OutlineEditor` (`reload_outline`, `rebuild_rows`, `run`, `flush`), `PageView` (`finish_load`, `enter_live`, `build_items`, `render`, `render_item`), query block load.
- **Hot spots**: `Outline::rows_reusing` + `rebuild_rows_after_command` (rows of unchanged blocks are reused after local commands); `Prebuilt` rows built in the page-load background task and adopted by `OutlineEditor::set_page_prebuilt`; live page reload no longer re-reads the whole page from SQLite (`PageView::show`); `search::TitleCache` in the reader pool (generation counter bumped by the writer after each job: `ReaderPool::generation`, `WriteConnection::bump_generation`); linked-reference breadcrumbs skipped for depth-1 hits (`read/refs.rs`).
- **Index benchmarks**: `tests/bench_large.rs` (540k blocks), `Spec::large/small`, `generate_big_page`, `generate_recent_journals`, `generate_app_bench_graph`; non-ignored CI smokes `search_latency_smoke_on_a_1000_page_graph` and `cold_build_smoke_is_fast_and_consistent`; test `cached_fuzzy_titles_follow_writes`.
- **Accessibility**: `contrast.rs` (WCAG ratio + tests over bundled themes; palette tuned in `assets/themes/bitacora.json`), `AppSettings.reduce_motion` (Settings > Appearance, `App::set_reduce_motion`, steady caret), `ui::a11y` + roles/labels (`TreeItem` rows, fold toggles, bullets, `Dialog`/`Heading` in `views/modal.rs`), `PaletteCommand::RenamePage`, Esc declines the credential prompt (`CredentialDialog::is_open`), keymap test `every_action_is_reachable_from_the_keyboard`.
- **i18n leftovers**: `Command::title()` translates the slash/angle menus (en, es; `editor.cmd.*`), notices `editor.date_unavailable`, `editor.no_templates`, `editor.drag_block`. Editor bullets already used `theme::bullet_color()` via the shared `block_view` row renderer.

## Why
1.0 targets: warm start < 1 s, page open < 100 ms, 16.7 ms frames, search p95, large graph numbers, WCAG AA, keyboard-only use.

## Results (see report)
Warm first journal 386-410 ms; 540k-block cold build 9.7 s, search p95 93 ms, 3-ref AND query 18 ms; indent/outdent on a 5,000-block page p95 21 -> 11 ms; live page UI-thread load 13 -> 2 ms; scroll CPU p99 5.8 ms; keystroke p95 4.5 ms.

## Verification
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `typos`, `cargo xtask check-deps` clean; `cargo test --workspace --locked --no-fail-fast`: 1538 passed, 0 failed (7 ignored benchmarks); benchmarks run in release on Xvfb+lavapipe.

## Open
Per-OS screen-reader validation; macOS/Windows/real-GPU frame times; paginate linked references for tag pages with tens of thousands of hits; modal focus traps; CI bench-job summary.
