---
id: BIT-US-0160
type: story
title: Export graph as SVG and PNG
status: done
priority: low
parent: BIT-EP-0024
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-graph]
estimate: 3
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T11:30:55Z
closed: 2026-10-07T11:30:55Z
---

## Description
As a user, I want to export the graph picture.

## Acceptance Criteria
- SVG writer from node/edge/position data (current filters, colours, labels optional).
- PNG via `tiny-skia` rasterisation of the same scene (licence checked with `cargo deny`), or SVG-only if rejected.
- Save dialog; atomic write.

## Notes
Implements BIT-SP-0012.R6. GPUI has no offscreen render (`render_to_image` stub, gpui-pre `platform.rs:1351`).
