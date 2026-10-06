---
id: BIT-T-0235
type: task
title: InsertBlocks planner with empty-target replacement and uuid policy; wire Mod+V and Mod+Shift+V
status: backlog
priority: high
parent: BIT-US-0037
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, clipboard]
estimate: 3
created: 2026-10-06T14:31:34Z
updated: 2026-10-06T14:31:34Z
---

## Description
`crates/bitacora-core/src/commands/insert_blocks.rs`: `plan_insert_blocks(target, sibling, blocks, keep_uuids)` → `InsertSubtree`×n (+ `RemoveSubtree` when the target is empty and edited, replacing it). `keep_uuids` true only for private payload from a cut; otherwise `id::` lines are stripped. Reject a uuid that already exists in the graph (generate a new one). Wire `Mod+V` (classifier → planner or inline buffer insert) and `Mod+Shift+V` (raw inline).

## Acceptance Criteria
- Golden tests: list paste into empty block, paste after non-empty block as siblings, cut-paste keeps `id::`, copy-paste drops `id::`.
- Paste is one undo step and restores bytes.

## Notes
Story BIT-US-0037. Implements BIT-SP-0004.R13. Logseq `core.cljs:520-593`, `editor.cljs:2013-2078`. ADR-006.
