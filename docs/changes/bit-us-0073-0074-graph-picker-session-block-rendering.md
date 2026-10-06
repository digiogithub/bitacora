---
created_at: 2026-10-06T18:44:49.099751169Z
updated_at: 2026-10-06T18:44:49.099751169Z
---
# Graph picker, index session and read-only block rendering (BIT-US-0073, BIT-US-0074)

Continues [[bitacora-full-development-plan]]; builds on [[bit-us-0014-0024-0025-app-shell]], [[bit-us-0006-0007-index-writer-and-reconcile]] and [[bit-us-0083-0084-0059-inline-scanner-tasks-page-props]]. Designs: [[block-editor]], [[04-editor-outliner-operations]], [[gpui-and-gpui-kit]].

## What changed (crates/bitacora-app, commit b8f4b75)
- `recent.rs`: `RecentGraphs` (max 10, atomic JSON in config dir, tolerant load), `has_graph_config`.
- `session.rs`: `GraphSession::start` opens the index (outside the graph), runs `Indexer::reconcile` on a std thread, emits `SessionEvent::{Opened{rebuilt,total}, Progress, Ready, Failed}` over `async_channel`; `close()` joins and shuts the indexer down, drop only signals. `initial_page` picks the page shown after opening (`--page`, newest journal, first page).
- `views/picker.rs` (`GraphPicker`, native folder dialog via `prompt_for_paths`, recent list, forget), `views/status_bar.rs` (`StatusEvent::IndexProgress`, progress bar), `views/workspace.rs` (`open_graph`, session event pump with toasts, graph switcher shows the picker, `--graph` skips it; no-`config.edn` warning toast), `ui::notify` tolerates windows without a Root.
- `render/inline.rs` (inline scanner tokens + own emphasis pass to `TextLayout` with styled and clickable ranges, images with `{:width}`), `render/model.rs` (`BlockModel`/`PageModel` from `block::analyze`: marker checkbox, priority, planning chips, properties table minus hidden built-ins, logbook summary, headings, collapsed, code/quote regions), `render/highlight.rs` (lexical highlighter), `views/page_view.rs` (virtualized `list`, `InteractiveText` clicks emit `PageEvent::Navigate`, async local images via `img`, `resolve_asset` confined to the graph), `panels.rs` (PageHost panel hosts the shared `PageView` through `SharedPageView` global).
- CLI `--page`; i18n keys `page.*`, `picker.*`, `status.index.progress`; facade exports (`Disableable`, `StyledImage`, `Progress`, `InteractiveText`, `img`, `PathPromptOptions`, ...).

## Why
Opening a graph with visible index progress and reading blocks like Logseq are the base of EP-0006; page navigation (BIT-US-0075) and editing build on these views.

## Verification
`cargo fmt`, `cargo clippy --workspace --all-targets --locked -D warnings`, `cargo test -p bitacora-app --locked` (118 pass, incl. session cold/warm/corrupt/missing, picker, workspace open flow, 387-fixture render smoke), `cargo xtask check-deps`, `cargo machete`, `typos`: clean. Screenshots under Xvfb+lavapipe (picker, demo page with all constructs).

## Notes / follow-ups
- Block refs resolve only inside the page being shown (second model pass); cross-page resolution needs the index read API (US-0008). A `BlockResolver` trait is the seam.
- pulldown-cmark (ADR-003) not used: block bodies are rendered inline-only (lists/tables inside bodies are shown as text). Remote images are placeholders. Code highlighting is lexical, no syntax crate.
- Page/block navigation events are logged only (BIT-US-0075); URLs open externally.
- The native folder dialog could not be exercised headless.
- Traces on BIT-SP-0003.R1/R6 were replaced then restored with the index crate paths merged in; their old `verified` stamp may need re-verification.