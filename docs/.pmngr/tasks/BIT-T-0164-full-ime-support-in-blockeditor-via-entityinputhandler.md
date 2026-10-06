---
id: BIT-T-0164
type: task
title: Full IME support in BlockEditor via EntityInputHandler marked text
status: done
priority: high
parent: BIT-US-0031
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor, ime]
estimate: 3
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
Complete `EntityInputHandler` for `BlockEditor`: `marked_text_range`, `replace_and_mark_text_in_range`, `unmark_text`, `bounds_for_range` (candidate window at caret), `character_index_for_point`, UTF-16 ↔ UTF-8 offset conversion. While composing, outliner key actions (Enter, Backspace, Tab, arrows) are not dispatched and the buffer is not committed; composition end commits normally.

## Acceptance Criteria
- `#[gpui::test]` simulating marked-text sequences: `nihongo` → `日本語`, then Enter splits.
- UTF-16 conversion unit tests with emoji and CJK.
- Candidate window bounds equal caret rect.

## Notes
Story BIT-US-0031. Implements BIT-SP-0004.R20. Reuse findings of the BIT-EP-0002 IME spike.
