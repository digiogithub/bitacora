# BIT-US-0173: Tasks list "disappearing" after date changes

Part of [[2-0-x-owner-feedback-plan]].

## Root cause
`TasksView` keeps its pill `Selection` (marker class, priority, page) across reloads. Rescheduling or
completing the last task a pill selects (for example the last overdue one with the Overdue pill active)
left a filter keeping zero rows while the model still held tasks, so the list rendered empty until the
app was restarted (fresh selection). The index and `task_groups` were verified to return all rows after
repeated SCHEDULED rewrites (no index-level bug).

## Changes
- `crates/bitacora-app/src/views/tasks.rs`
  - `Selection::reconciled(&TaskModel)`: drops page/priority/marker dimensions that match nothing; applied
    in `TasksView::reload` on every successful load.
  - `TaskModel::from_groups`: a block with an unknown marker is skipped with a `warn!` (never empties the list).
  - `reload`: on error the previous rows are kept (log only).
  - Test `rescheduling_the_last_overdue_task_resets_the_stale_overdue_filter`.

## Verification
`RUSTFLAGS="-L <scratchpad>/lib" cargo test -p bitacora-app --locked --lib views::tasks` (7 passed).
