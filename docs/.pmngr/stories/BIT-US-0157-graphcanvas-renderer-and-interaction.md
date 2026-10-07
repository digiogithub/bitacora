---
id: BIT-US-0157
type: story
title: GraphCanvas renderer and interaction
status: backlog
priority: high
parent: BIT-EP-0024
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-app]
estimate: 8
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T09:19:46Z
---

## Description
As a user, I want to explore my graph visually: a dark canvas with nodes sized by connections, pan, zoom, hover highlight and click to open pages.

## Acceptance Criteria
- `GraphCanvas` view via `canvas()`: nodes as rounded `paint_quad`s (size `8*max(1,cbrt(degree))`, token colours for normal/current/tag), edges batched per colour in one `PathBuilder::stroke` + `paint_path`, viewport culling.
- Pan (drag empty space), zoom at cursor (`ScrollWheelEvent`), node drag reheats, hover highlights neighbours and dims others, click (no drag) opens page, labels above zoom threshold.
- `request_animation_frame` only while simulation alive or interacting; idle CPU when settled; smooth at 3k nodes. Reached from sidebar "Graph" nav item.

## Notes
Implements BIT-SP-0012.R2, BIT-SP-0012.R3, BIT-SP-0012.R4. APIs: gpui-pre 0.3.8 `elements/canvas.rs:10`, `window.rs:4550,4621,5157,5294,2670`, `path_builder.rs`.
