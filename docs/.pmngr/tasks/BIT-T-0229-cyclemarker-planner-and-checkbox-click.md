---
id: BIT-T-0229
type: task
title: CycleMarker planner and checkbox click
status: done
priority: high
parent: BIT-US-0035
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, tasks]
estimate: 2
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
`crates/bitacora-core/src/commands/marker.rs`: `plan_cycle_marker(ids, workflow)` edits only the leading marker word: `:todo` → `TODO→DOING→DONE→none→TODO`; `:now` → `LATER→NOW→DONE→none→LATER` (other markers such as WAITING/CANCELED map to the workflow's first state). `plan_set_done(id, bool)` for checkbox clicks. Workflow from `config.edn :preferred-workflow`. Wire `Mod+Enter` in editor and selection contexts and checkbox click in the renderer.

## Acceptance Criteria
- Table tests over both workflows incl. priority `[#A]` and properties preserved.
- Multi-selection cycles in one transaction.

## Notes
Story BIT-US-0035. Implements BIT-SP-0004.R14. Logseq `util/marker.cljs:40-73`, `editor.cljs:708-757`. LOGBOOK time tracking is out of scope (later).
