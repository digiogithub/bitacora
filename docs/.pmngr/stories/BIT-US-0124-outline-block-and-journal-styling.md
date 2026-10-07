---
id: BIT-US-0124
type: story
title: Outline block and journal styling
status: in_review
priority: high
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, editor, bitacora-app]
estimate: 8
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T11:16:13Z
started: 2026-10-07T11:16:13Z
---

## Description
As a user, I want blocks and journals to look like the design: bullets and guide lines, editing background, task chips/markers, Literata display titles, journal header with "Today" pill, folded ring, 760px reading column.

## Acceptance Criteria
- Editor, DnD, slash and IME tests pass unchanged; no change to parse/serialize (round-trip fixtures untouched).
- Dark/light screenshot review against `mockups/Main.dc.html` and `Claro.dc.html` recorded.

## Notes
Files: block editor views in `crates/bitacora-app/src/views/`. Implements BIT-SP-0008.R1.
