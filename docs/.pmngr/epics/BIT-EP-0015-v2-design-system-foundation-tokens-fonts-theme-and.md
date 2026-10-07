---
id: BIT-EP-0015
type: epic
title: "v2 design system foundation: tokens, fonts, theme and component kit"
status: backlog
priority: high
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, design-system, bitacora-app]
created: 2026-10-07T09:10:26Z
updated: 2026-10-07T09:10:26Z
---

## Description
Adopt the Bitacora design system (`/www/Bitacora/bitacora-design-system`) as the single source of UI values: vendored `tokens.json` + generator + contrast check, generated palette, embedded fonts (Atkinson Hyperlegible Next/Mono, Literata), `BitacoraTheme` global (TypeScale, Metrics, mode) ported to `gpui-pre 0.3.8` via `gpui_kit::gpui`, gpui-kit theme bridge ("Bitacora Dark/Light"), reusable components in `crate::ui`, and a lint test against magic values.

## Acceptance Criteria
- BIT-SP-0008.R1, R2, R3, R6 satisfied and verified.
- Existing theme features (System/Light/Dark, user themes, `custom.css`, font settings) keep working.
- Facade rule (`only_ui_names_gpui_kit`) still passes.

## Notes
- Plan [[bitacora-v2-plan]] decision D6 (ADR-032).
- Design-system docs are in Spanish; our code/docs in English.
- `rust/theme.rs` in the design system targets upstream gpui: port, verify every API against the pinned fork.
