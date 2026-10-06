---
id: BIT-T-0198
type: task
title: Right sidebar Dock panel with item stack
status: done
priority: medium
parent: BIT-US-0080
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:52:51Z
started: 2026-10-06T19:14:39Z
closed: 2026-10-06T19:52:51Z
---

## Description
`crates/bitacora-app/src/views/right_sidebar.rs`: Dock panel (`DockArea`, right side, `Resizable`) holding `Vec<SidebarItem::{Page(name), Block(uuid), References(page)}>`, each a `Collapsible` card with close button, rendering `PageView`/subtree read-only. Shift+click on page refs, block refs and bullets dispatches `OpenInSidebar`. Persist stack + width per graph via Dock JSON persistence.

## Acceptance Criteria
- `#[gpui::test]`: Shift+click on a ref adds an item on top; close removes; restart restores stack.
- Items refresh on index events.

## Notes
[[gpui-and-gpui-kit]] §2.2 (Dock, Resizable, Collapsible).
