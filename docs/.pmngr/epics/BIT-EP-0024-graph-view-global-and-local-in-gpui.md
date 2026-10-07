---
id: BIT-EP-0024
type: epic
title: Graph view (global and local) in GPUI
status: backlog
priority: medium
milestone: BIT-M-0009
author: mcp
labels: [v2, ui, graph, bitacora-graph, bitacora-index, bitacora-app]
created: 2026-10-07T09:10:54Z
updated: 2026-10-07T09:10:54Z
---

## Description
Logseq-like graph view: graph data read API in `bitacora-index`, in-house force layout in a new GPUI-free crate `bitacora-graph` (Barnes-Hut, Logseq defaults) running on a background thread, `GraphCanvas` renderer (`canvas()` + `paint_quad` + batched `PathBuilder` edges), pan/zoom/hover/click, settings panel (Nodes/Search/Forces/Export) persisted in config.edn, local graph in the right panel, focus/N-hops, incremental refresh, SVG/PNG export. Reached from the "Graph" sidebar nav item of the new design.

## Acceptance Criteria
- BIT-SP-0012.R1-R6 satisfied and verified.
- Smooth at 3k nodes, idle CPU when settled.

## Notes
- Plan [[bitacora-v2-plan]] decision D8 (ADR-034). Logseq behaviour refs (read only, ADR-015): `handler/graph.cljs:84-175`, `components/page.cljs:577-770`, `extensions/graph/pixi.cljs:59-100`, config template `config.edn:288-296`.
- GPUI APIs (gpui-pre 0.3.8): `elements/canvas.rs:10`, `window.rs:4550` paint_quad, `:4621` paint_path, `:2670` request_animation_frame, `path_builder.rs`.
