---
id: BIT-T-0157
type: task
title: Merge confirmation dialog in rename UI
status: done
parent: BIT-US-0087
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, rename]
estimate: 2
created: 2026-10-06T14:30:30Z
updated: 2026-10-06T21:54:02Z
closed: 2026-10-06T21:54:02Z
---

## Description
In `crates/bitacora-app/src/views/page_title.rs` (rename flow), when the planner returns `Merge`, show a GPUI Kit confirmation dialog: "Page 'Bar' already exists. Merge 'Foo' into it?" listing block counts and aliases that would be dropped. Cancel leaves everything unchanged.

## Acceptance Criteria
- `#[gpui::test]`: confirm triggers merge transaction; cancel issues no ops.
- Dialog lists dropped aliases.

## Notes
BIT-SP-0002.R13. ADR-001.
