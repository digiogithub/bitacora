---
id: BIT-US-0115
type: story
title: BitacoraTheme global with type scale and metrics
status: backlog
priority: high
parent: BIT-EP-0015
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, bitacora-app, theme]
estimate: 5
created: 2026-10-07T09:11:26Z
updated: 2026-10-07T09:11:26Z
---

## Description
As a developer, I want a `BitacoraTheme` global (palette, `TypeScale`, `Metrics`, mode) reachable as `cx.bitacora()`, so that views use design values by name.

## Acceptance Criteria
- Port of design-system `rust/theme.rs` to `gpui-pre 0.3.8` via `gpui_kit::gpui`, under `crates/bitacora-app/src/ui/theme/`, exposed through the `crate::ui` facade.
- Mode follows the existing `ThemePreference` (System/Light/Dark); a print variant (always light) exists for PDF.
- `#[gpui::test]`: toggling mode updates the global; every type style and metric resolves.

## Notes
Implements BIT-SP-0008.R1. Existing `crates/bitacora-app/src/theme.rs` (ThemeController) remains the entry point.
