---
id: BIT-US-0150
type: story
title: Attach page or selection as chat context
status: done
priority: medium
parent: BIT-EP-0022
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, agui, privacy, bitacora-app]
estimate: 3
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T12:26:01Z
started: 2026-10-07T12:17:11Z
closed: 2026-10-07T12:26:01Z
---

## Description
As a user, I want to explicitly attach the current page or selected blocks to a question, so the agent focuses on them and nothing else is sent.

## Acceptance Criteria
- Context chips in the composer (current page, selection, journal day); "Ask about selection" command from block context menu.
- Attached content passes `ContentPolicy`; `RunAgentInput.context` contains only attached items (test).

## Notes
Implements BIT-SP-0011.R1.
