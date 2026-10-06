---
id: BIT-US-0078
type: story
title: Search and command palette with keyboard-only navigation
status: in_progress
priority: high
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, search, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T19:14:39Z
started: 2026-10-06T19:14:39Z
---

## Description
As a keyboard user, I want to press Mod+K, type, and jump to a page or block with the arrow keys and Enter, so that I never need the mouse to navigate.

## Acceptance Criteria
- Mod+K opens a GPUI Kit `Command` palette fed by `bitacora-index` search (pages and blocks sections, highlighted snippets).
- Results update as-you-type with debounce ≤ 50 ms and cancel stale queries.
- Enter opens; Shift+Enter opens in the right sidebar; Esc closes; scope chips: "This page", "Journals", "Pages".
- Mod+Shift+P opens an actions palette (go to journals, all pages, toggle sidebars, reindex).
- Fully operable without the mouse (focus trap, screen-reader labels).

## Notes
Implements: BIT-SP-0003.R14. [[gpui-and-gpui-kit]] §2.2 (Command, List). [[sqlite-index-schema]] §6.
