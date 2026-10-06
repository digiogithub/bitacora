---
id: BIT-US-0050
type: story
title: Field-level block merge with automatic metadata resolution
status: backlog
priority: critical
parent: BIT-EP-0012
milestone: BIT-M-0004
author: mcp
labels: [merge, sync, metadata]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T14:28:30Z
---

## Description
As a user editing on several devices, I want metadata differences (collapsed state, ids, clocks, flashcard state, property order, whitespace) to resolve silently and only genuine text collisions to be flagged, so that I'm never nagged about noise.

## Acceptance Criteria
- Content: 3-way with in-block line diff3; overlap → `CONFLICT(content)` with ours kept in output.
- User properties per key; same key different values → `CONFLICT(property)`.
- Metadata rules per ADR-009: `collapsed::` ours-first, `id::` union with referenced-wins + `((uuid))` rewrite, LOGBOOK union, `card-*` whole group by later `card-last-reviewed`, other metadata keys LWW, property order ours + appended theirs.
- Task marker and `SCHEDULED`/`DEADLINE` rules.
- Table-driven tests: one case per row of [[git-sync-merge]] §4.3.

## Notes
Implements: BIT-SP-0006.R10, BIT-SP-0006.R11, BIT-SP-0006.R13. See [[git-sync-merge]] §4.3, [[02-markdown-block-syntax]] §5.3–5.4. ADR-009.
