---
id: BIT-US-0080
type: story
title: "Right sidebar: open pages and blocks with Shift+click"
status: done
priority: medium
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, bitacora-app]
estimate: 3
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T19:52:58Z
started: 2026-10-06T19:14:39Z
closed: 2026-10-06T19:52:58Z
---

## Description
As a reader, I want to Shift+click a page ref, block ref or bullet to open it in a right sidebar stack, so that I can read two things side by side.

## Acceptance Criteria
- Right sidebar is a resizable Dock panel holding a stack of items (page, block subtree, page references), each collapsible and closable.
- `t r` toggles it; the stack and width persist per graph across restarts.
- Items refresh on index events like the main page view.

## Notes
[[04-editor-outliner-operations]] §9 (right sidebar); [[gpui-and-gpui-kit]] §2.2 (Dock, Resizable).
