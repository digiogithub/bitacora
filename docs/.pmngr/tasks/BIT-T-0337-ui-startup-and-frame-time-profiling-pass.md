---
id: BIT-T-0337
type: task
title: UI startup and frame-time profiling pass
status: done
priority: medium
parent: BIT-US-0109
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, performance]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-07T08:20:53Z
started: 2026-10-06T22:47:18Z
closed: 2026-10-07T08:20:53Z
---

## Description
Instrument `bitacora-app` with `tracing` spans (startup phases, index open, first journal render, page open, search palette round-trip) and an opt-in frame-time overlay (`gpui-fps`). Profile on the medium and large graphs on Linux, macOS and Windows; fix the top hotspots (e.g. cache shaped text runs per block, avoid re-parsing inline AST on every render, prefetch next outline page).

## Acceptance Criteria
- Warm start to first journal render < 1 s (medium graph); page open < 100 ms; scrolling a 5,000-block page with p99 frame time < 16.7 ms on the reference machines; report attached to the PR.

## Notes
[[gpui-and-gpui-kit]] §1.10, §2.3 ("Measure the performance of a page with 5k blocks").
