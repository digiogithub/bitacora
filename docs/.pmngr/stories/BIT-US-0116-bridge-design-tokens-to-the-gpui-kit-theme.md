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
updated: 2026-10-07T09:54:04Z
---

## Description
As a user, I want kit widgets (inputs, lists, docks, scrollbars) to use the new palette, so that the whole app is consistent.

## Acceptance Criteria
- Bundled "Bitacora Dark" and "Bitacora Light" kit themes are generated from tokens, using the mapping in design-system `docs/gpui-kit.md`. Each field is verified against gpui-component 0.7.1 `ThemeColor`.
- They replace the 1.x "Paper" theme, which is removed (owner decision 2026-10-07). Users who had Paper or a custom theme selected fall back to the Bitacora theme for their mode.
- `logseq/custom.css` overrides keep working as in 1.x. User theme files are not part of v2: a future colour-scheme config file format is planned post-2.0.
- Existing theme tests are updated and pass. A screenshot review in both modes is recorded.

## Notes
Implements BIT-SP-0008.R1. Files: `crates/bitacora-app/assets/themes/`, `theme.rs`, `custom_css.rs`.
