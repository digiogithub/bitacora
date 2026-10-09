---
id: BIT-US-0187
type: story
title: "Tasks view: working checkboxes, open goes to the page, click \"No date\" to schedule"
status: in_review
priority: high
author: mcp
labels: [bug, bitacora-app, tasks, feedback]
created: 2026-10-09T12:52:29Z
updated: 2026-10-09T13:05:07Z
started: 2026-10-09T13:05:07Z
---

## Description
Owner report (2026-10-09) on the Tasks view (`crates/bitacora-app/src/views/tasks.rs`):
1. Task checkboxes do nothing.
2. The open button zooms into the block alone; it should open the containing page (the parent context, normally the page) with the task block scrolled into view and focused/highlighted, so it can be edited in context.
3. The "No date" label on unscheduled tasks should be clickable and open the date picker to schedule it (writes `SCHEDULED: <date>` through the core queue, like the existing reschedule action).

## Acceptance Criteria
- Clicking a checkbox toggles TODO/DOING→DONE (and DONE→TODO where shown) via the core command queue; the row updates/moves after reindex.
- Open navigates to the page containing the block, scrolls to and highlights/edits the block; modifiers (Shift → side pane, Ctrl/Cmd → new tab) keep working.
- "No date" opens the same date picker used elsewhere; choosing a date schedules the task; Escape cancels.
- Tests for each behaviour (view logic / gpui tests).
