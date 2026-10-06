---
id: BIT-T-0188
type: task
title: Navigation history with back/forward and scroll restore
status: done
priority: medium
parent: BIT-US-0075
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:08:15Z
closed: 2026-10-06T19:08:15Z
---

## Description
`crates/bitacora-app/src/navigation.rs`: `Navigator` global with a history stack of `Route::{Journals, Page(name), Block(uuid), AllPages}` + saved scroll anchor; actions `GoBack` (Mod+[) and `GoForward` (Mod+]) registered in the keymap; mouse back/forward buttons; toolbar arrows. Max 100 entries.

## Acceptance Criteria
- Unit tests for stack semantics (navigate after back truncates forward).
- `#[gpui::test]`: back restores the previous page and its scroll anchor.

## Notes
[[04-editor-outliner-operations]] §7 (shortcuts).
