---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - index
    - v2
---
# Tasks view (BIT-US-0126)

Continues [[bitacora-v2-plan]]; stories BIT-US-0126, tasks BIT-T-0402 and BIT-T-0403.

## What changed
- `bitacora-index` (`read/misc.rs`): `TaskGroup`, `TaskGroups`, `TaskItem::due()`, `IndexReader::task_groups(today, &TaskFilter)` (open tasks, bucketed Overdue / This week = today..today+6 / Later / No date by the earliest of SCHEDULED and DEADLINE) and `IndexReader::overdue_count(today)`. Test: `tests/read_misc.rs::task_groups_bucket_open_tasks_by_earliest_date`.
- `bitacora-app`: new `views/tasks.rs` (`TasksView`, `TaskModel`, `Selection`, `MarkerClass`, `apply_action`). Column capped at the `tasks_max` metric (860px), kit `Pill` filters with counts (marker class, priority A/B/C, page via the "in <page>" link), kit `Overline` group headers (overdue in `warn`), kit `Card` (Panel surface) rows with kit `TaskMarker`. Row click opens the block (Shift = right sidebar), the checkbox completes it (`Cmd::ToggleDone`), the marker cycles it (`Cmd::CycleMarker`); both run through the core command queue as undoable transactions.
- New `Route::Tasks` (nav.rs, main_view.rs, sidebar/right_sidebar/page_view exhaustive matches) and palette command `GoTasks` ("Go to: Tasks"); locale keys `tasks.*` in en/es.

## Why
Design `mockups/Tareas.dc.html`; no tasks screen existed. No sidebar entry was added here (the sidebar is being restyled by another story): `Route::Tasks` is reachable from the command palette and `MainView::navigate`.

## Verification
`cargo test -p bitacora-index --test read_misc`, `cargo test -p bitacora-app tasks`, clippy `-D warnings` on both crates. Grouping and counts are asserted on a fixture graph; completion, cycle and undo are asserted against a real session.
