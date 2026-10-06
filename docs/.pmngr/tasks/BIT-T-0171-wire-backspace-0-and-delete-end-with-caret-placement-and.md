---
id: BIT-T-0171
type: task
title: Wire Backspace@0 and Delete@end with caret placement and refusal notices
status: backlog
priority: high
parent: BIT-US-0033
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor]
estimate: 2
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
In `BlockEditor`, `Backspace` with caret 0 and empty selection (after autopair check) dispatches `MergeWithPrevious`; `Delete` at end dispatches `MergeNext`. On success edit mode moves to the survivor with caret at the junction; on `Refusal` show a transient toast with the reason. Golden tests in `crates/bitacora-core/tests/cmd_merge.rs` with undo byte-exact assertions.

## Acceptance Criteria
- `#[gpui::test]`: caret ends at offset 3 after merging `foo`+`bar`.
- ≥ 8 golden cases incl. children adoption and CRLF; undo byte-exact.

## Notes
Story BIT-US-0033. Implements BIT-SP-0004.R7.
