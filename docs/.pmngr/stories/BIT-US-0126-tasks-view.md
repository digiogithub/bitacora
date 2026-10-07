---
id: BIT-US-0126
type: story
title: Tasks view
status: todo
priority: medium
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-app, bitacora-index]
estimate: 8
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T11:10:03Z
---

## Description
As a user, I want a Tasks view (`mockups/Tareas.dc.html`): tasks grouped Overdue / This week / Later / No date, filter pills with counts (marker, priority, page), 860px column, rows that open or toggle the block.

## Acceptance Criteria
- Uses the existing index task queries (SCHEDULED/DEADLINE, markers); counts match a fixture graph test.
- Toggling a marker goes through the core Op queue (undoable); works in dark and light.

## Notes
Index: `crates/bitacora-index` task/query DSL.
