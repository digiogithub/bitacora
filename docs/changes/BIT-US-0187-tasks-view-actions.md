# BIT-US-0187: Tasks view actions (checkbox, open in context, undated tasks)

Continues [[BIT-US-0173-tasks-stale-filter]] and [[2-0-x-owner-feedback-plan]].

## Root causes and changes
1. **Checkboxes did nothing.** Row element ids were `("tasks-check", task.ord)`. `ord` is the block position
   in its file, so it repeats across pages (every page has an `ord` 1). GPUI keys click state by element id, so
   rows with equal ids share it: the mouse-up of a non-hovered duplicate consumed the pending click and the
   hovered checkbox never fired `on_click`. The core path (`Cmd::ToggleDone`, the editor's own toggle, DONE <->
   workflow start) was already correct and tested. Ids (marker, checkbox, title, page link, open link, planning
   chip) now derive from the block uuid. Regression test `clicking_the_checkbox_completes_the_task` clicks four
   rows over two pages with equal ords and checks the files.
2. **Open button zoomed into the block alone.** New `Route::PageAt { page, block }` and
   `NavTarget::PageAt { page, block }` (nav.rs, render/inline.rs), handled in main_view, workspace (main, new
   tab), right_sidebar (Shift), sidebar, top_bar, print_ui, graph_state (stored as a plain page). `PageView`
   keeps `reveal: Option<String>`; `render` defers `reveal_block` once loaded: resolves the row (`reveal_row`,
   by uuid or by index ord minus the pre-block offset from core's snapshot), opens collapsed ancestors
   (`collapsed_ancestors`, view only), `focus_row` (highlight), scrolls, and `click_row` puts the block in edit
   mode on a live page. Tasks `open` emits `open_target(task)`; Shift and Ctrl/Cmd modifiers keep working.
3. **"No date".** For editable rows the label is a `PlanningChipView` (SCHEDULED, empty timestamp) with the
   shared picker; picking reuses `RowAction::Reschedule` -> `move_planning_date`, which adds
   `SCHEDULED: <YYYY-MM-DD Ddd>`. The picker opens on the current month (`picker_month`; it used 1970 for no
   date). Escape/outside click close it through the existing `PopoverShell` dismiss.

## Files / symbols
`crates/bitacora-app/src/views/tasks.rs` (ids, `open_target`, `picker_month`, no-date chip),
`views/page_view.rs` (`reveal`, `reveal_block`, `reveal_row`, `collapsed_ancestors`, `Route::PageAt` in `show`),
`nav.rs`, `render/inline.rs`, `graph_state.rs`, `views/{main_view,workspace,right_sidebar,sidebar}.rs`,
`views/workspace/{top_bar,print_ui}.rs`.

## Verification
`cargo test -p bitacora-app --locked` (680 passed), `cargo clippy -p bitacora-app --all-targets --locked -- -D
warnings`, `cargo fmt --all`. New tests: checkbox click, picker month, SCHEDULED added to an undated task,
open target, reveal helpers, `page_at_scrolls_to_the_block_highlights_and_edits_it`.
Not verified visually: highlight colour, scroll position, and the picker popover in the real app.
