# BIT-US-0178: tasks panel under today's journal

Continues [[2-0-x-owner-feedback-plan]].

## What changed
- New `crates/bitacora-app/src/views/today_panel.rs`: `build_sections` (pure: agenda items plus
  DOING/NOW tasks into "Scheduled today", "Tomorrow", "Doing"; Doing wins over date sections),
  `load_sections` (index reads `agenda(today, 1)` and `tasks(markers DOING/NOW)`, run off the UI
  thread), and the `TodayPanel` view (marker pill, text, page title; click navigates to the block;
  `on_index_changed` reloads debounced). Empty sections are not drawn.
- `views/journals.rs`: the panel entity is created in `show`, drawn at the end of today's entry,
  reloaded on index events. `views/page_view.rs`: `Item::Today` after the blocks when the page is
  today's journal.
- `bitacora-index` `IndexReader::add_days` made public.
- i18n: `journals.panel_today/panel_tomorrow/panel_doing` (en, es).

## Verification
`cargo test -p bitacora-app -p bitacora-index --locked` (630 app tests pass), clippy `-D warnings`
on both crates. Tests: `sections_split_by_day_and_doing_wins`, `empty_sections_are_empty`.
Visuals not checked (no display).
