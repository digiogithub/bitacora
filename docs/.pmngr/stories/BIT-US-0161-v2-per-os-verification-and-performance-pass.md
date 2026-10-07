---
id: BIT-US-0161
type: story
title: v2 per-OS verification and performance pass
status: in_review
priority: high
parent: BIT-EP-0025
milestone: BIT-M-0009
author: mcp
labels: [v2, release, qa]
estimate: 5
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T13:43:47Z
started: 2026-10-07T13:23:42Z
---

## Description
As the release owner, I want v2 verified on every OS and within performance budgets before tagging 2.0.

## Acceptance Criteria
- Manual checklists run on Linux (GNOME/KDE Wayland, X11), macOS, Windows: frameless, fonts, IME (incl. ghost text guard), Pando setup/consent/chat/review, graph view.
- Performance: startup, page open, search p95 (hybrid and lexical), graph view fps at 3k nodes, semantic sync throughput on large preset; regressions vs ADR-026 numbers fixed or accepted with ADR.

## Notes
Reuses `perf` module (`--perf-bench`), `docs/design/ime-test-checklist.md`, frameless checklist.
