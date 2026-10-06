---
created_at: 2026-10-06T22:30:47.701653211Z
updated_at: 2026-10-06T22:30:47.701653211Z
---
# BIT-US-0105 / BIT-US-0106: slash and angle commands, templates, block drag and drop

Continues [[bitacora-full-development-plan]], [[block-editor]], [[04-editor-outliner-operations]].

## US-0105
- app: `editor/commands.rs` (catalogue, fuzzy filter, pure edits), `editor/view/slash.rs` (command run, template insertion, calendar popup, upload hook), `completion.rs` Trigger::Slash/Angle, Item::Command/Template, `element.rs` popup window + calendar, `ui::calendar` facade (chrono dep), keymap DatePicker context.
- core: `TemplateContext` (today/yesterday/tomorrow/time/current page/[[Page]]), `Cmd::InsertTemplate`, `Workspace::template_blocks/template_names`.
- index: `IndexReader::templates()`.
- app: `graph_ops::ensure_today_templated` loads the journal template page before ensuring today.
- Known: `<% time %>` in the default journal template stays literal (no clock in core at journal creation).

## US-0106
- app: `editor/dnd.rs`, `editor/view/dnd.rs`, `RowDrag` (row.rs), `views/block_view.rs` hooks, `EditorEvent::Scroll`, EditorRegistry peer refresh.
- core: `Cmd::DropBlockRef { sources, target }`; cross-page `MoveBlocks` already one transaction.

## Verification
fmt, clippy workspace -D warnings clean; workspace tests 1513 passed 0 failed. New: gpui tests slash_tests (11), dnd_tests (6), core templates_and_drops (9), index templates. Xvfb+lavapipe+XTest: slash popup, calendar click writes SCHEDULED, template list insertion, bullet drag with indicator and preview.