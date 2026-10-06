---
id: BIT-T-0232
type: task
title: DeleteBlocks planner and bulk ops dispatch on selection
status: backlog
priority: high
parent: BIT-US-0036
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, outliner]
estimate: 2
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T14:31:33Z
---

## Description
`crates/bitacora-core/src/commands/delete.rs`: `plan_delete_blocks(ids)` → `RemoveSubtree` per top-level block (reference inlining is out of MVP scope; referenced blocks get a warning toast). Dispatch `Backspace/Delete`, `Tab`, `Shift+Tab`, `Alt+Shift+Up/Down`, `Mod+Enter` in `BlockSelection` context to the existing planners with the top-level ids, one transaction each. After delete, select the previous visible block.

## Acceptance Criteria
- Golden test: delete `a`(with child) and `b` → `- c`; one undo restores bytes.
- Bulk indent of `a`, `a1`, `b` indents only top-level blocks.

## Notes
Story BIT-US-0036. Implements BIT-SP-0004.R12. Logseq `editor.cljs:3140`, `core.cljs:654-707`.
