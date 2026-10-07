---
id: BIT-M-0003
type: milestone
title: M2 — Editor MVP (MVP-β)
status: done
author: mcp
created: 2026-10-06T14:20:19Z
updated: 2026-10-07T08:21:50Z
started: 2026-10-07T00:15:15Z
closed: 2026-10-07T08:21:50Z
due: 2027-04-15
---

## Description
Block editing with the core Logseq keyboard model, undo/redo, byte-preserving atomic writes, external-change handling, autocomplete, page create/rename/delete, assets, and MCP write tools through the same op pipeline.

## Acceptance Criteria
- A graph edited with Bitacora opens in Logseq 0.10.x without differences other than the edited blocks.
- All MVP editing features listed in [[block-editor]] §9 work on the 3 OSes.
- External edits (Logseq open on the same graph) are merged or surfaced, never lost.

## Notes
See [[architecture]] §6.
