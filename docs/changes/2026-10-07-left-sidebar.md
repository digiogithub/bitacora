---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - index
    - ui
---
# Left sidebar: nav, calendar, favorites/recents, status footer (BIT-US-0123)

Part of [[bitacora-v2-plan]]; uses the component kit ([[component-kit]]).

## What changed

- `crates/bitacora-app/src/views/sidebar.rs`: `LeftSidebar` rewritten as the 252px (`metrics.sidebar_left`) `side` column with `line` right border. Nav (Journals, All pages, Tasks with overdue count in `warn` mono, Graph; 36px rows, radius 8, active = `hover` fill + weight 600), calendar, Favorites / Recent (`Overline` + 30px rows, Shift+click opens the right sidebar) and a footer (graph name + page count, MCP dot + transport, sync dot). New: `Target::{Tasks, Graph}`, `SidebarEvent::{OpenJournalDay, GoToDate}`, `set_handle`, `on_index_changed` (400ms debounce), `set_footer_status`, `set_selected_day`, `set_active_target`, `shift_month`, `click_day`, pure `row_look` / `endpoint_label` / `status_dot`.
- `crates/bitacora-app/src/views/calendar.rs` (new): pure `month_grid` (Monday first), `shift_month`, `month_bounds`, `cell_look` (selected > today > future muted, 4px dot, room always reserved) and `render_calendar`.
- `crates/bitacora-app/src/data.rs`: `SidebarData`, `sidebar_data()`.
- `crates/bitacora-index/src/read/misc.rs`: `IndexReader::journal_days_with_notes(from, to)`, `overdue_task_count(today)`, `file_page_count()` (no schema change).
- `views/workspace.rs` (minimal): sidebar gets the graph handle on open/close, index events, selected journal day on `Visited`, footer status mirrored from the status bar (`sync_sidebar_footer`), handling of the new sidebar events.
- Locales `sidebar.*` and `calendar.*` (en, es).

## Behaviour notes

- Clicking a day navigates to `Route::Page(journal title)`; a journal without a file stays virtual until edited (no creation on click).
- "Go to date..." opens the command palette (no date-jump command exists yet).
- `Target::Tasks` / `Target::Graph` only highlight: `Workspace::on_sidebar_event` has an empty arm to wire to the Tasks (BIT-US-0126) and graph view routes when they land. Use `LeftSidebar::set_active_target` to highlight them.
- The MCP client count of the design is not shown: the runtime does not expose it.

## Verification

`cargo test -p bitacora-index --locked` (2 new tests in `tests/read_misc.rs`), `cargo test -p bitacora-app --locked` (447 lib tests incl. calendar maths, cell looks in both modes, sidebar width/nav/click events), `cargo clippy -p bitacora-app -p bitacora-index --all-targets --locked -- -D warnings` clean.
