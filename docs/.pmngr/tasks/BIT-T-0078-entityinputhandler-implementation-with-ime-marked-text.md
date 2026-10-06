---
id: BIT-T-0078
type: task
title: EntityInputHandler implementation with IME marked text
status: in_progress
priority: critical
parent: BIT-US-0040
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, ime, bitacora-app]
estimate: 3
created: 2026-10-06T14:28:47Z
updated: 2026-10-06T17:09:41Z
started: 2026-10-06T17:09:41Z
---

## Description
Implement `EntityInputHandler` for `SpikeBlockEditor`: `text_for_range`, `selected_text_range`, `marked_text_range`, `unmark_text`, `replace_text_in_range`, `replace_and_mark_text_in_range`, `bounds_for_range`. Provide `utf16_to_utf8(&str, usize)` / `utf8_to_utf16` helpers with unit tests (ASCII, CJK, emoji with surrogate pairs, combining marks). Register `ElementInputHandler::new(bounds, entity)` via `window.handle_input(&focus_handle, ..)` in `paint`. `bounds_for_range` must return the on-screen rect of the given range on the correct wrapped row so IME candidate windows follow the caret.

## Acceptance Criteria
- Unit tests for offset conversion pass, including a surrogate-pair emoji and a string with combining accents.
- `#[gpui::test]` simulating `replace_and_mark_text_in_range` then `replace_text_in_range` yields the committed text and clears the marked range.
- Manual: macOS Japanese input shows the candidate window at the caret (screenshot in the spike report).

## Notes
- [[gpui-and-gpui-kit]] §1.2 (EntityInputHandler, UTF-16 offsets), §3.2 item 7; [[block-editor]] §7.2.
