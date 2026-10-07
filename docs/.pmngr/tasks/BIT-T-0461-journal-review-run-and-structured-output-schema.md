---
id: BIT-T-0461
type: task
title: Journal review run and structured output schema
status: done
priority: medium
parent: BIT-US-0151
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, journal, bitacora-pando]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T11:51:01Z
closed: 2026-10-07T11:51:01Z
---

## Description
Run `bitacora-journal-reviewer` for a date range; JSON schema (summary, themes[], mood, pending_tasks[block_uuid], next_actions[]); validation and cross-check of tasks with the index query.

## Acceptance Criteria
- Tests with recorded runs: valid output parsed; invalid output surfaced as error; tasks not in index dropped.
