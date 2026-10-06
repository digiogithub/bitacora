---
id: BIT-T-0081
type: task
title: Clipboard and basic in-block undo for the spike editor
status: backlog
priority: medium
parent: BIT-US-0040
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, bitacora-app]
estimate: 2
created: 2026-10-06T14:28:47Z
updated: 2026-10-06T14:28:47Z
---

## Description
Add `Copy`, `Cut`, `Paste`, `SelectAll` to `SpikeBlockEditor` using `cx.write_to_clipboard(ClipboardItem::new_string(..))` / `read_from_clipboard`. Paste of multi-line text inserts it inline in the spike (multi-block paste is BIT-EP-0007). Add a naive per-editor undo stack (coalesce typing by 500 ms) only to measure feel; document in the spike report that production undo is the page-level op log in `bitacora-core` ([[block-editor]] §4).

## Acceptance Criteria
- Copy/cut/paste round trip works with system clipboard on all 3 OSes (manual) and in `#[gpui::test]` (test platform clipboard).
- Undo after typing a word restores the previous text.

## Notes
- [[gpui-and-gpui-kit]] §3.3 (clipboard APIs), §3.2 item 6; [[block-editor]] §4, §7.5.
