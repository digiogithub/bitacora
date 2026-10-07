---
id: BIT-T-0401
type: task
title: Right panel shell with Context and Agent tabs
status: backlog
priority: medium
parent: BIT-US-0125
milestone: BIT-M-0006
author: mcp
labels: [v2, ui]
estimate: 3
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T09:13:45Z
---

## Description
360px panel with `Tab` header (Context / Agent), toggle from top bar, persisted open state and active tab; Context hosts refs/properties and named slots (related blocks, recommendations, local graph); Agent hosts restyled `agent_activity` and a slot for Pando chat.

## Acceptance Criteria
- `#[gpui::test]` for tab switching and persistence; existing agent activity tests pass.
