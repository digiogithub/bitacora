---
created_at: 2026-10-07T11:00:00Z
updated_at: 2026-10-07T11:00:00Z
tags:
    - design
    - graph
    - app
---
# Graph view (renderer, interaction, local graph, incremental refresh)

Implements BIT-US-0157 and BIT-US-0159 on top of ADR-034 ([[bitacora-v2-plan]] "Graph view", BIT-SP-0012.R2-R4 and R6). Data comes from `IndexReader::graph_data` / `local_graph_data`; layout from `bitacora-graph`.

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

## Open questions / follow-ups
- Settings panel and config.edn persistence (BIT-US-0158) and export (BIT-US-0160) plug into `GraphView::set_settings` and the paint data.
- Node ceiling: the 3k target is covered by culling and batched paths; 20k with LOD is not attempted.
- Manual check on real hardware (smoothness at 3k nodes, idle CPU) is not covered by automated tests.
