---
id: BIT-US-0151
type: story
title: Journal review
status: backlog
priority: medium
parent: BIT-EP-0023
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, journal, bitacora-app, bitacora-pando]
estimate: 8
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T09:18:16Z
---

## Description
As a journaling user, I want Pando to review a day or a week of my journal: summary, recurring themes, mood, pending tasks and suggested next actions.

## Acceptance Criteria
- "Review today"/"Review this week" from the journal header and Agent tab; profile `bitacora-journal-reviewer` reads journals via MCP; structured JSON output validated.
- Review card (amber AI style) with summary, themes, mood indicators, pending tasks cross-checked with the index task query, next actions; stored in a Bitacora-owned cache, re-runnable.
- Optional user schedule (e.g. daily at 21:00) and "Insert into journal" via `propose_edit` approval.

## Notes
Implements BIT-SP-0011.R4. Owner open question: storage location of reviews (cache by default).
