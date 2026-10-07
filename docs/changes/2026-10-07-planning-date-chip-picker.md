---
created_at: 2026-10-07T17:40:00Z
updated_at: 2026-10-07T17:40:00Z
tags:
    - change
    - editor
    - tasks
---
# Editable SCHEDULED / DEADLINE date chips (BIT-US-0167)

Continues [[bitacora-v2-plan]] (epic BIT-EP-0017). Tasks: BIT-T-0496..0499.

## What changed
- `bitacora-markdown` `edit/state.rs`: `move_planning_date(content, keyword, y, m, d)` rewrites only the `YYYY-MM-DD Ddd` bytes of an existing `SCHEDULED:` / `DEADLINE:` entry (repeaters such as `.+1d`, times, bracket kind and the rest of the block stay byte for byte; a missing line is created like `set_scheduled`). `clear_planning(content, keyword)` removes the entry (the whole line, or only its segment when two entries share a line). Tests: `tests/planning_move.rs`.
- `bitacora-app` `views/planning.rs`: `PlanningChipView` (RenderOnce) draws the chip and, when open, a `PopoverShell` in `deferred(anchored())` inside the chip with `views::calendar::render_calendar` and a "Remove date" button. `PlanningActions` carries the host callbacks; the chip owns no state.
- Outline: `RowEdit.planning` (`editor/row.rs`), `OutlineEditor::{open,close,shift,pick}_planning`, `clear_planning_date` (`editor/view/planning.rs`). Picking is one `Cmd::SetText` (single undo step); a block in edit mode flushes first and leaves edit mode. `block_view.rs` renders chips through `PlanningChipView`.
- Tasks view: `RowAction::Reschedule { keyword, day }` in `apply_action` (core queue, undoable); the due text is now a chip with the same picker (`TasksView::{open,shift,close}_planning`, `reschedule`).
- Locale keys `planning.*` (en, es).
- References: they get the chip picker as soon as their rows are built with `RowEdit` (`OutlineEditor::row_edit` includes `planning`). Rows drawn with `edit: None` keep a static chip.

## Slash commands
`/scheduled` and `/deadline` already existed (`editor/commands.rs`, `editor/view/slash.rs`) and match Logseq 0.10.15 `commands.cljs` (clear the slash text, show the date picker; the planning line sits right under the first line). New gpui test covers replace + undo. Not implemented: the picker time and repeater inputs of Logseq (existing repeaters are preserved by the chip, but `/scheduled` writes a plain timestamp like Logseq does when no repeater is chosen).

## Verification
`cargo test -p bitacora-markdown`, `cargo test -p bitacora-app --lib` (planning_tests, tasks tests), clippy `-D warnings`.
