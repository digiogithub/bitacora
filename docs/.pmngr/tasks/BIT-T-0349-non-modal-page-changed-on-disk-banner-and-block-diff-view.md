---
id: BIT-T-0349
type: task
title: Non-modal "Page changed on disk" banner and block diff view
status: done
priority: high
parent: BIT-US-0070
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, ui]
estimate: 3
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T20:37:17Z
started: 2026-10-06T19:59:19Z
closed: 2026-10-06T20:37:17Z
---

## Description
`crates/bitacora-app/src/views/conflict_banner.rs`: GPUI Kit banner at the top of `PageView` when the page is conflicted, with [Keep mine (overwrite)], [Take disk version], [Show diff]. Diff view: side-by-side block list (disk vs mine) with added/removed/changed highlighting from `ConflictDiff`. Editing continues to work while conflicted (changes stay in memory). Status-bar indicator lists conflicted pages.

## Acceptance Criteria
- `#[gpui::test]`: banner appears on `Notice::PageConflict`, buttons dispatch the right commands, banner disappears after resolution.
- Banner is non-modal: other pages can be opened and edited.

## Notes
Story BIT-US-0070. Implements BIT-SP-0005.R16. [[block-editor]] §6.2, §9 item 14.
