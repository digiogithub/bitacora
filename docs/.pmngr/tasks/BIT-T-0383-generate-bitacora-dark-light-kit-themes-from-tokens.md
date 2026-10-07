---
id: BIT-T-0383
type: task
title: Generate Bitacora Dark/Light kit themes from tokens
status: in_review
priority: high
parent: BIT-US-0116
milestone: BIT-M-0006
author: mcp
labels: [v2, theme, xtask]
estimate: 3
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T10:40:54Z
started: 2026-10-07T10:40:54Z
---

## Description
Extend `xtask tokens` to emit the kit theme JSON (`assets/themes/bitacora-v2.json`) using the `docs/gpui-kit.md` mapping; verify each field name against gpui-component 0.7.1 `theme/theme_color.rs`; make it the default theme.

## Acceptance Criteria
- Kit widgets (input, list, dock, scrollbar, popover) show token colours in both modes; theme loader tests pass.
