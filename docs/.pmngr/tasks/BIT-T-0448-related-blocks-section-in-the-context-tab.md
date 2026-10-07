---
id: BIT-T-0448
type: task
title: Related blocks section in the Context tab
status: in_review
priority: medium
parent: BIT-US-0145
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-app]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T12:23:17Z
started: 2026-10-07T12:23:17Z
---

## Description
For the focused block/page, query semantic search with its text (debounced), exclude itself, list related blocks with page breadcrumbs; click navigates.

## Acceptance Criteria
- `#[gpui::test]`; no request when feature disabled or no consent.
