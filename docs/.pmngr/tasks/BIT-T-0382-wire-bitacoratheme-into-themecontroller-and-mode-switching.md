---
id: BIT-T-0382
type: task
title: Wire BitacoraTheme into ThemeController and mode switching
status: backlog
priority: high
parent: BIT-US-0115
milestone: BIT-M-0006
author: mcp
labels: [v2, theme, bitacora-app]
estimate: 2
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
`theme::apply` sets both the kit Theme and the `BitacoraTheme` global; System/Light/Dark preference and OS appearance changes update it; print variant available for PDF export.

## Acceptance Criteria
- `#[gpui::test]`: switching preference updates `cx.bitacora().mode` and palette; window re-renders.
