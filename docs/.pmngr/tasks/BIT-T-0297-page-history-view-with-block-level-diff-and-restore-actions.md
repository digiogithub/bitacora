---
id: BIT-T-0297
type: task
title: Page history view with block-level diff and restore actions
status: in_progress
priority: low
parent: BIT-US-0048
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, history, ui]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T19:59:19Z
started: 2026-10-06T19:59:19Z
---

## Description
`crates/bitacora-app/src/views/page_history.rs`: list of versions; selecting one runs the merge crate's block matcher (2-way: version vs current) to show added/removed/changed blocks with word diff; toggle "show metadata changes" (hidden by default, classification from ADR-009); "Restore block" inserts/replaces via core op (position by former parent/left sibling, else parent end); "Restore page" replaces page content as one undo transaction.

## Acceptance Criteria
- `#[gpui::test]` for diff classification display and restore op generation.

## Notes
Story BIT-US-0048. Implements BIT-SP-0006.R22. Reuses BIT-US-0049 matcher.
