---
id: BIT-T-0338
type: task
title: "Accessibility audit: keyboard reachability, focus rings and contrast"
status: in_review
priority: medium
parent: BIT-US-0109
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, accessibility]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-07T00:10:21Z
started: 2026-10-06T22:47:18Z
---

## Description
Audit every view for keyboard reachability (Tab order, focus traps in dialogs/palettes, Esc behaviour), visible focus rings using theme tokens, contrast ≥ WCAG AA (4.5:1 text) for bundled themes (automated check over theme JSON in a unit test), reduced-motion setting honoured, and accessibility labels/roles wherever GPUI exposes them. Document known gaps (GPUI screen reader support) in `docs/analysis/rust/gpui-and-gpui-kit.md` §1.10.

## Acceptance Criteria
- Checklist in the PR covering all views; contrast unit test passes for all bundled themes.
- No action in the actions palette is mouse-only.

## Notes
[[gpui-and-gpui-kit]] §1.10.
