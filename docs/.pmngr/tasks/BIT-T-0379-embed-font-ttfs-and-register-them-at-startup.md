---
id: BIT-T-0379
type: task
title: Embed font TTFs and register them at startup
status: backlog
priority: high
parent: BIT-US-0114
milestone: BIT-M-0006
author: mcp
labels: [v2, fonts, bitacora-app]
estimate: 2
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
Add the static TTFs to `crates/bitacora-app/assets/fonts/` with OFL licence files, register them via `text_system().add_fonts` in app init before opening windows, set Atkinson Hyperlegible Next as default UI font and wire Mono/Literata into `TypeScale`.

## Acceptance Criteria
- App renders with embedded fonts on a machine without them installed; bundle size increase noted in the KB change record.
