---
created_at: 2026-10-07T11:30:00Z
updated_at: 2026-10-07T11:30:00Z
tags:
    - change
    - app
    - graph
---
# Graph view: canvas renderer, interaction, local graph, incremental refresh

Continues [[bitacora-v2-plan]] (D8 / ADR-034) and the layout engine of BIT-US-0156. Stories BIT-US-0157 (BIT-T-0475, 0476, 0477) and BIT-US-0159 (BIT-T-0480, 0481, 0482). Design: [[graph-view]].

## What changed
- `bitacora-graph`: `Simulation::with_positions(input, params, seed, initial, alpha)` starts selected nodes at given positions and at a chosen alpha (used by refreshes). Test `with_positions_keeps_given_starts_and_alpha`.
- `bitacora-app` (new dep `bitacora-graph`, allowed by `cargo xtask check-deps`):
  - `src/graph_view/model.rs`: `GraphModel` (GraphData -> layout input, adjacency, `within_hops`, `restrict`), `carry_positions`, `node_radius`.
  - `src/graph_view/viewport.rs`: `Viewport` (pan, zoom at cursor, fit, culling), `hit_test`.
  - `src/views/graph_view.rs`: `GraphView` (global + local modes), `GraphSettings` seam, `canvas()` painting with batched `PathBuilder` edges and `paint_quad` circles, labels, frame lifecycle, pan / zoom / drag-pin / hover dim / click-to-open, modifier-click focus + N hops toolbar, debounced incremental refresh preserving positions.
  - `src/ui/mod.rs`: `ui::canvas` facade (paint + pointer event types).
  - Entry points: `Route::Graph` (`nav.rs`, `MainView::graph`), sidebar `Target::Graph`, palette `PaletteCommand::GoGraph`, action `GoGraph` (`g g`), locale keys `graph_view.*`, `sidebar.graph_view`, `palette.cmd_graph_view` (en, es).
  - Local graph: card at the top of `RightSidebar` (`set_local_page`, `local_graph`), driven by `MainEvent::Visited` in `workspace.rs`.
- Docs: `docs/design/graph-view.md`.

## Why
Visual exploration of the graph (BIT-SP-0012.R2-R4, R6) without jumping layouts on edits.

## Verification
- `cargo test -p bitacora-graph -p bitacora-app`: 14 + 438 passed (20 new graph tests: model, viewport, view logic through `#[gpui_test]`, right sidebar local graph).
- `cargo clippy -p bitacora-graph -p bitacora-app --all-targets -- -D warnings` clean; `cargo xtask check-deps` OK.
- Not verified: visual smoothness at 3k nodes and idle CPU on real hardware (manual).
