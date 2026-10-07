---
id: BIT-T-0381
type: task
title: Port TypeScale, Metrics and BitacoraTheme to gpui-pre
status: backlog
priority: high
parent: BIT-US-0115
milestone: BIT-M-0006
author: mcp
labels: [v2, theme, bitacora-app]
estimate: 3
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
Write `ui/theme/{mod,type_scale,metrics}.rs` from the design-system `rust/theme.rs` reference (type scale display 34 … overline 11.5, spacing 2…56, radii 5…999, layout widths 52/252/760/860/360), verified against the pinned `gpui-pre 0.3.8` APIs; expose `cx.bitacora()` through the facade.

## Acceptance Criteria
- Compiles with clippy `-D warnings`; unit tests for metrics and type styles.
