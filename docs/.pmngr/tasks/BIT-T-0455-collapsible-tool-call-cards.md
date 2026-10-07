---
id: BIT-T-0455
type: task
title: Collapsible tool-call cards
status: backlog
priority: medium
parent: BIT-US-0148
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-app]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T09:19:06Z
---

## Description
Card component for tool calls (mono font, name, streamed args, result preview, status, duration), collapsed by default; reasoning blocks collapsed; activity progress line.

## Acceptance Criteria
- `#[gpui::test]` for card states (running, ok, error).
