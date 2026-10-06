---
id: BIT-T-0139
type: task
title: Click-to-caret mapping from rendered inline runs to source offsets
status: backlog
priority: critical
parent: BIT-US-0030
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor, ui]
estimate: 3
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T14:30:00Z
---

## Description
Extend the inline renderer (`crates/bitacora-app/src/render/inline.rs`) to record, for every rendered run, its source byte range in the visible projection. On mouse-down over rendered content (not on links/checkbox/bullet), map glyph index → source offset; hidden markup (`[[`, `]]`, `**`, `((uuid))` rendered as title) snaps to the nearest valid offset. Enter edit mode with that caret.

## Acceptance Criteria
- Unit tests: `Meet [[Alice]] today` click inside `Alice` → offset within `Alice`; click on bold text maps inside `**…**`; click on a rendered block ref snaps to after `))`.
- Clicking a link or checkbox does not enter edit mode.

## Notes
Story BIT-US-0030. Implements BIT-SP-0004.R1. Logseq ref `components/block.cljs:2200-2203`.
