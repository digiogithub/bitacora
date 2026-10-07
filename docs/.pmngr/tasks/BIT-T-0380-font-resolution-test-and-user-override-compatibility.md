---
id: BIT-T-0380
type: task
title: Font resolution test and user override compatibility
status: backlog
priority: high
parent: BIT-US-0114
milestone: BIT-M-0006
author: mcp
labels: [v2, fonts, tests]
estimate: 1
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
`#[gpui::test]` that the three families resolve after registration (including the "Literata 36pt" fallback name); verify `font_family`/`font_size` settings and `custom.css` font overrides still apply.

## Acceptance Criteria
- Tests green on Linux CI; manual check on macOS/Windows listed in the release checklist.
