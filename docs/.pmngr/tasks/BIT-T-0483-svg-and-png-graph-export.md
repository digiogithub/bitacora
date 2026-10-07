---
id: BIT-T-0483
type: task
title: SVG and PNG graph export
status: done
priority: low
parent: BIT-US-0160
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-graph]
estimate: 3
created: 2026-10-07T09:20:35Z
updated: 2026-10-07T11:30:55Z
closed: 2026-10-07T11:30:55Z
---

## Description
Scene description → SVG writer; optional `tiny-skia` PNG rasteriser (after `cargo deny`); save dialog, atomic write.

## Acceptance Criteria
- SVG test: one circle per visible node, one line per visible edge; PNG dimensions/scale test.
