---
id: BIT-T-0192
type: task
title: Reference filters popover (include/exclude pages)
status: backlog
priority: medium
parent: BIT-US-0077
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, references]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T14:30:56Z
---

## Description
Filter button in the linked references header opens a `Popover` listing the pages co-referenced in the results with counts; click = include, Shift+click = exclude; active filters shown as `Tag`s. Read existing `filters::` page property as initial state; in this epic filters are kept in memory (writing `filters::` is done once editing exists, BIT-EP-0007).

## Acceptance Criteria
- `#[gpui::test]`: excluding a page removes the matching groups; clearing restores them.

## Notes
BIT-SP-0003.R9. [[04-editor-outliner-operations]] §9.
