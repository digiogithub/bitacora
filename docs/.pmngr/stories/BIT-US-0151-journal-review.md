---
id: BIT-US-0151
type: story
title: Journal review
status: in_review
priority: medium
parent: BIT-EP-0023
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, journal, bitacora-app, bitacora-pando]
estimate: 8
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T12:54:55Z
started: 2026-10-07T12:54:55Z
---

## Description
As a journaling user, I want Pando to review a day or a week of my journal: summary, recurring themes, mood, pending tasks and suggested next actions.

## Acceptance Criteria
- "Review today" and "Review this week" are available from the journal header and the Agent tab. The `bitacora-journal-reviewer` profile reads journals via MCP, and its structured JSON output is validated.
- A review card (amber AI style) shows the summary, themes, mood indicators, pending tasks cross-checked with the index task query, and next actions.
- Reviews are stored **only in a machine-local cache** (owner decision 2026-10-07) and can be re-run.
- Optional user schedule (e.g. daily at 21:00). "Insert into journal" goes through a `propose_edit` approval.

## Notes
Implements BIT-SP-0011.R4.
