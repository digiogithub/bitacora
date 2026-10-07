---
id: BIT-T-0398
type: task
title: Calendar widget and journal-days index query
status: backlog
priority: high
parent: BIT-US-0123
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-index]
estimate: 5
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T09:13:45Z
---

## Description
Month calendar component (Monday-first, prev/next month, today/selected styles, dots for days with non-empty journals) and an index read API returning journal days with content for a month; click navigates to the journal day.

## Acceptance Criteria
- Index test over a fixture graph; month-grid unit tests (leap years, week start); click opens the correct journal.
