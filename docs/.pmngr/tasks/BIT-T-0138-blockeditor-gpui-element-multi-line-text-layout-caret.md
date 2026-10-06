---
id: BIT-T-0138
type: task
title: "BlockEditor GPUI element: multi-line text layout, caret, selection and soft wrap"
status: done
priority: critical
parent: BIT-US-0030
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor, ui]
estimate: 5
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
In `crates/bitacora-app/src/editor/block_editor.rs` build the custom `BlockEditor` entity editing `EditProjection.visible`: `TextLayout`/`WrappedLine` soft wrap, caret and selection painting, mouse selection, word/line movement (`Alt+F/B`, Home/End), `EntityInputHandler` basic text replacement (IME polish in BIT-US-0031). Integrate with `PageView` so exactly one row renders the editor and others render inline runs.

## Acceptance Criteria
- `#[gpui::test]`: typing, selection replace, Backspace/Delete within the buffer, word movement.
- Only one block in edit mode at a time; switching blocks commits the previous one.
- Blocks > 10,000 chars are selected instead of edited.

## Notes
Story BIT-US-0030. Implements BIT-SP-0004.R1. ADR-001 (via `gpui_kit::gpui`), ADR-002. No GPL Zed code (ADR-014).
