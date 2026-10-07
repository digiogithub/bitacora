---
id: BIT-US-0118
type: story
title: Guard against magic UI values and document the token pipeline
status: backlog
priority: medium
parent: BIT-EP-0015
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, bitacora-app, docs]
estimate: 2
created: 2026-10-07T09:11:26Z
updated: 2026-10-07T09:11:26Z
---

## Description
As a maintainer, I want a test that rejects raw colours/pixel literals in views and an ADR for the tokens pipeline, so that the design stays the single source of truth.

## Acceptance Criteria
- Test (like `only_ui_names_gpui_kit`) fails on hex colour or inline `px(` literals in `views/` outside an allowlist.
- ADR-032 row in `docs/architecture.md`; `docs/design/design-system.md` describes tokens, fonts, components and amber/accent semantics.

## Notes
Implements BIT-SP-0008.R1, BIT-SP-0008.R6. Plan [[bitacora-v2-plan]] D6.
