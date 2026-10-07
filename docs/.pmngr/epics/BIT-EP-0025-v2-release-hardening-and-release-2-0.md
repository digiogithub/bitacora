---
id: BIT-EP-0025
type: epic
title: v2 release hardening and Release 2.0
status: backlog
priority: medium
milestone: BIT-M-0009
author: mcp
labels: [v2, release]
created: 2026-10-07T09:10:54Z
updated: 2026-10-07T09:10:54Z
---

## Description
Ship 2.0: per-OS manual verification (frameless, IME, fonts, Pando flows), performance pass for the new UI and semantic sync, settings migration from 1.x, user documentation for v2 features (design, Pando setup, privacy, graph view), release notes, and a tagged 2.0 release through the existing pipeline.

## Acceptance Criteria
- Manual checklists signed for Linux (Wayland + X11), macOS and Windows.
- 1.x settings and themes migrate without loss; 2.0 artifacts pass smoke tests.

## Notes
- Plan [[bitacora-v2-plan]]. Reuses the packaging/release pipeline from BIT-EP-0014.
