---
id: BIT-T-0167
type: task
title: Wire Enter and Shift+Enter in BlockEditor including special cases
status: backlog
priority: critical
parent: BIT-US-0032
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor]
estimate: 2
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
In `crates/bitacora-app/src/editor/actions.rs` handle `editor::NewBlock`: inside a code fence → insert `\n`; caret inside `[[…]]`/`((…))` with no popup → move past the closing marker; empty last child → `OutdentEmptyLast`; otherwise `SplitBlock`. `editor::NewLine` inserts `\n`. After split, edit mode moves to the new block with caret at 0.

## Acceptance Criteria
- `#[gpui::test]`: split, insert-before, code-fence newline, Shift+Enter.
- Edit focus and caret follow `cursor_after`.

## Notes
Story BIT-US-0032. Implements BIT-SP-0004.R6.
