---
id: BIT-US-0116
type: story
title: Bridge design tokens to the gpui-kit theme
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
As a user, I want kit widgets (inputs, lists, docks, scrollbars) to use the new palette, so that the whole app is consistent.

## Acceptance Criteria
- Bundled "Bitacora Dark" and "Bitacora Light" kit themes generated from tokens using the mapping in design-system `docs/gpui-kit.md`, verified against gpui-component 0.7.1 `ThemeColor` fields.
- They become the default; the 1.x "Paper" theme stays selectable; user theme files and `logseq/custom.css` overrides keep their precedence.
- Existing theme tests pass; screenshot review in both modes recorded.

## Notes
Implements BIT-SP-0008.R1. Files: `crates/bitacora-app/assets/themes/`, `theme.rs`, `custom_css.rs`.
