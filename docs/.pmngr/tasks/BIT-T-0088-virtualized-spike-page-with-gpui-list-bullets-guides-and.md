---
id: BIT-T-0088
type: task
title: Virtualized spike page with gpui::list, bullets, guides and folding
status: in_progress
priority: high
parent: BIT-US-0060
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, bitacora-app]
estimate: 3
created: 2026-10-06T14:29:14Z
updated: 2026-10-06T17:33:08Z
started: 2026-10-06T17:33:08Z
---

## Description
Extend `SpikePage` to render a flattened DFS row list of visible blocks with `gpui::list` + `ListState` (variable heights measured lazily). Row = gutter (collapse arrow, bullet, children-count badge when collapsed) + content (rendered `StyledText` or the live editor); indentation guides per depth. Data sources: a generator (`--spike-blocks N`, default 1,000, random depth/length incl. multi-line and CJK) and a loader that naively splits a fixture page by `- ` bullets. Folding toggles a block's `collapsed` flag and applies `ListState::splice` only on the affected row range; keep the editing block's state when it scrolls out of view.

## Acceptance Criteria
- 1,000 and 10,000 generated blocks scroll without visual glitches; editing a block near the bottom keeps the list position stable.
- Unit test for DFS flattening + splice range computation on collapse/expand.

## Notes
- [[block-editor]] §7.1; [[gpui-and-gpui-kit]] §1.1 (Lists), §2.3 (block outline editor gap).
