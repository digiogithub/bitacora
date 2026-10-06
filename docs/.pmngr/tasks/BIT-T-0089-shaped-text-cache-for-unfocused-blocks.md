---
id: BIT-T-0089
type: task
title: Shaped-text cache for unfocused blocks
status: backlog
priority: medium
parent: BIT-US-0060
milestone: BIT-M-0001
author: mcp
labels: [spike, performance, bitacora-app]
estimate: 2
created: 2026-10-06T14:29:14Z
updated: 2026-10-06T14:29:14Z
---

## Description
Cache the inline-render output (display string, runs, offset map) per `(block id, content hash)` and the shaped/wrapped lines per `(block id, content hash, width, theme generation)` in an LRU (size bound configurable, default 4,096 entries). Invalidate on content change, width change or theme change. Expose hit/miss counters in the debug overlay of the spike.

## Acceptance Criteria
- Scrolling back over already-seen rows shows > 90% cache hits in the counters.
- Theme switch invalidates and re-renders correctly (no stale colours).
- Before/after numbers for scroll frame time recorded in the spike report.

## Notes
- [[gpui-and-gpui-kit]] §3.2 item 4 ("Cache the shaped output per (block id, content hash, width)"), Risk R8.
