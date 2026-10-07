---
id: BIT-EP-0023
type: epic
title: Journal review, AI recommendations and AI writing surfaces
status: backlog
priority: medium
milestone: BIT-M-0008
author: mcp
labels: [v2, pando, ai, bitacora-app, bitacora-pando]
created: 2026-10-07T09:10:54Z
updated: 2026-10-07T09:10:54Z
---

## Description
Agentic features on top of the chat infrastructure: journal review (day/range summary, themes, mood, pending tasks, next actions; on demand and scheduled), recommendations (related pages, link and tag suggestions, next actions) shown as amber chips in the Context tab, inline AI ghost text and "Compose with AI" (⌘J) box, all inserting nothing without explicit user action.

## Acceptance Criteria
- BIT-SP-0011.R4, R5, R6 satisfied and verified.
- Each feature can be switched off independently in Pando settings.

## Notes
- Plan [[bitacora-v2-plan]]. Uses Pando profiles `journal-reviewer`, `recommender`, `writer` (shipped by the Pando server support epic).
