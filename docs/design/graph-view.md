---
created_at: 2026-10-07T11:00:00Z
updated_at: 2026-10-07T11:00:00Z
tags:
    - design
    - graph
    - app
---
# Graph view (renderer, interaction, local graph, incremental refresh)

Implements BIT-US-0157, BIT-US-0158, BIT-US-0159 and BIT-US-0160 on top of ADR-034 ([[bitacora-v2-plan]] "Graph view", BIT-SP-0012.R2-R4 and R6). Data comes from `IndexReader::graph_data` / `local_graph_data`; layout from `bitacora-graph`.

## Modules (`bitacora-app`)
- `src/graph_view/model.rs` (pure): `GraphModel::from_data(&GraphData)` (page ids -> layout indices, adjacency), `to_input()` (-> `bitacora_graph::GraphInput`), `within_hops(focus, n)` + `restrict(keep)` (focus filter, degrees recomputed), `carry_positions(old, old_positions, new)` (incremental refresh), `node_radius(degree) = 8 * max(1, cbrt(degree))`.
- `src/graph_view/viewport.rs` (pure): `Viewport` (`screen = world * zoom + offset`, relative to the canvas centre), `zoom_at` (keeps the point under the cursor fixed, clamped 0.05..8), `fit`, culling helpers, `hit_test`.
- `src/views/graph_view.rs`: `GraphView` entity (`GraphMode::Global` main route, `GraphMode::Local` widget), `GraphSettings` seam (filter + `ForceParams`; config.edn persistence is BIT-US-0158).

The GraphData -> GraphInput conversion lives in the app (allowed by the `check-deps` rules: `bitacora-app` may name `bitacora-graph`).

## Rendering
`canvas()` inside an `overflow_hidden` div: background quad, edges, nodes. Edges are buffered per colour (normal / touching the hovered node) and stroked as `PathBuilder::stroke` paths of at most 2000 segments (keeps tessellation inside the `u16` index range). Nodes are `paint_quad` circles (corner radius = radius), dimmed passes first. Segments and discs outside the canvas are culled. Labels are absolutely positioned text children (cap 160) shown above zoom 0.9, plus the hovered node, its neighbours and the current page. Colours come from `BitacoraTheme` (`cx.bitacora().colors`): `bg`, `text_2` nodes, `muted` journals, `ok` tags, `accent` current/hover/focus ring and highlighted edges, `line_2` edges. Hover dims everything except the node and its neighbours.

## Frame lifecycle
`render` pulls the latest `Snapshot`; it calls `window.request_animation_frame()` only while `wants_frames()` (snapshot not settled, a drag in progress, or a control message was sent and no settled snapshot newer than it has arrived). A settled layout costs no CPU; the 16 ms simulation tick runs on the worker thread. Until the user pans/zooms/drags, the viewport re-fits on every new snapshot.

## Interaction
Pointer logic takes canvas-centre-relative positions (`pointer_down/move/up`, `scroll`) so it is unit-testable. Drag on empty space pans; wheel zooms at the cursor; pressing a node and moving more than 3 px pins it (`Control::Pin`, which reheats) and release sends `Unpin`; press-release without movement is a click: opens the page (Shift = right sidebar) through `PageEvent`, or toggles the focus when Ctrl/Cmd is held.

## Focus and N hops (BIT-US-0159)
Modifier-click toggles a node in the focus set; the shown model is the focus nodes plus everything within N hops (toolbar "-" / "+" buttons, 1..6; a slider widget can replace them once the component kit lands) and "Reset focus" clears it. Focus changes rebuild the simulation with carried positions.

## Incremental refresh (BIT-SP-0012.R6)
Index events debounce 400 ms then reload `GraphData` on a background thread. `apply_data` rebuilds the model; if the shown model is unchanged nothing happens (no reheat). Otherwise `carry_positions` keeps the positions of surviving pages, places a new page 30 units from an already placed neighbour (deterministic angle from its id) and the new `Simulation::with_positions(.., alpha = 0.3)` reheats gently.

## Entry points and local graph
Route `Route::Graph`, sidebar item (`Target::Graph`), palette command `GoGraph`, action `bitacora::GoGraph` bound to `g g`. The local graph is a card at the top of the right sidebar stack (`RightSidebar::set_local_page`, driven by `MainEvent::Visited`); it is the stand-in for the Context tab until the right-panel redesign story lands (move the `GraphView` entity into that tab).

## Settings panel and persistence (BIT-US-0158)
The toolbar's gear button toggles a floating panel (`src/views/graph_view/panel.rs`, kit `Card`, `Button`, `Segmented`, `Input`) in the global graph, with four collapsible sections: Nodes (page and link counts, journals / orphan / built-in / excluded toggles, pause simulation), Search (a label filter: matching pages stay bright, the rest dims through the same `lit` mask the hover uses), Forces (link distance, charge strength, charge range as stepper rows over Logseq's slider ranges 10..180 / -1000..1000 / 500..4000, "Reset forces") and Export.

Keys are Logseq 0.10.x's (`components/page.cljs:577-700`, `handler/graph.cljs:84-125`): `:graph/settings {:journal? :orphan-pages? :builtin-pages? :excluded-pages?}` (defaults off / **on** / off / off; `:excluded-pages? true` *shows* pages with `exclude-from-graph-view:: true`, hence the new `GraphFilter::show_excluded`) and `:graph/forcesettings {:link-dist :charge-strength :charge-range}` (defaults 70 / -600 / 600, which are also our `ForceParams` defaults, so the values map one to one). Reading and writing live in `bitacora-config` (`graph_view.rs`: `GraphViewSettings`, `EffectiveConfig::graph_view_settings`, `ConfigEditor::{set_graph_toggle, set_graph_force, reset_graph_forces}`); a missing key or a value of the wrong type falls back to the default. "Reset forces" removes `:graph/forcesettings` (Logseq resets the values in memory only).

Every change updates the view at once (a filter change reloads `GraphData`; a force change starts a new `Simulation` with the carried positions) and is written through the single writer: `GraphEdit::{GraphToggle, GraphForce, GraphForcesReset}` -> `edit_config` -> `CommandQueue` (comment-preserving splice, atomic, hash-checked). Writes are serialised (`pending` / `saving`): edits made while one is in flight go out as one batch afterwards. The view gets the queue from `MainView::set_session_link` and reads the saved settings once per graph in `show` (the handle only carries the config as it was at open time). None of the graph keys feed `config_hash`, so saving them never triggers a reindex. The local graph widget keeps the defaults.

## Export (BIT-US-0160)
`src/graph_view/export.rs` is GPUI-free: a `Scene` (nodes with position, radius, label and colour; edges; palette) built from the shown `GraphModel` and the current layout positions, so exports follow the active filters and focus. `Scene::to_svg` writes the SVG by hand (background rect, edge lines, circles, escaped `<text>` labels); `Scene::to_png` rasterises the same scene with `tiny-skia` 0.11.4 (already in the lockfile through gpui -> resvg, BSD-3-Clause, `cargo deny check` passes, no new crate) at 2x, shrunk so no side exceeds 8192 px. `tiny-skia` has no text shaping, so **the PNG has no labels** (the SVG does; the panel says so). The panel's buttons open the platform save dialog (`cx.prompt_for_new_path`), append the extension when missing and write atomically with `settings::write_atomic` on a background thread; the result or the error shows in the panel.

## Open questions / follow-ups
- PNG labels would need a text rasteriser (`resvg` with its `text` feature plus a font database); not added because it pulls in `fontdb`/`rustybuzz`-class dependencies for little value.
- The settings use stepper buttons instead of sliders: the kit has no slider yet.
- Node ceiling: the 3k target is covered by culling and batched paths; 20k with LOD is not attempted. Measured recommendation (owner's call): keep 5,000 nodes as the 2.0 ceiling; layout tick is 3.4 ms at 5k and 13.6 ms at 20k (see [[performance-v2]] section 2.4).
- Manual check on real hardware (smoothness at 3k nodes, idle CPU) is not covered by automated tests.
