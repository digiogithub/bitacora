---
id: BIT-T-0402
type: task
title: Tasks view grouping, filter pills and row actions
status: in_review
priority: medium
parent: BIT-US-0126
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-index]
estimate: 5
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T11:21:49Z
started: 2026-10-07T11:21:34Z
---

## Description
New `views/tasks.rs`: query tasks via the index (markers, SCHEDULED/DEADLINE), group Overdue / This week / Later / No date, filter pills with counts, row click opens block, marker toggle as an Op.

## Acceptance Criteria
- Fixture-graph test for grouping and counts; marker toggle undoable.
