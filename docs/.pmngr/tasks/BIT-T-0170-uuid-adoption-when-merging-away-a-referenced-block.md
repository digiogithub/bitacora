---
id: BIT-T-0170
type: task
title: uuid adoption when merging away a referenced block
status: backlog
priority: critical
parent: BIT-US-0033
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, outliner]
estimate: 2
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
Extend merge planners: query the graph index (`GraphIndex::is_referenced(uuid)`) for the removed block's `id::`; if referenced and the survivor has no referenced uuid, the survivor's new text gets the `id:: <uuid>` line (replacing an unreferenced own `id::` if any); if both are referenced → `Refusal("both blocks are referenced")`. Unreferenced `id::` of the removed block is dropped.

## Acceptance Criteria
- Test: after merge, `((uuid))` in another page resolves to the survivor; no other file is modified.
- Undo restores both `id::` lines byte-exactly.

## Notes
Story BIT-US-0033. Implements BIT-SP-0004.R8. ADR-006. Logseq `editor.cljs:851-856`.
