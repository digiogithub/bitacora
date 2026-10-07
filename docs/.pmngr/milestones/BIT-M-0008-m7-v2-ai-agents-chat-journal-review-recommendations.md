---
id: BIT-M-0008
type: milestone
title: "M7 — v2 AI agents: chat, journal review, recommendations"
status: backlog
author: mcp
labels: [v2, pando, ai]
created: 2026-10-07T09:07:50Z
updated: 2026-10-07T09:07:50Z
due: 2027-05-14
---

## Description
Pando agents inside Bitacora over AG-UI: a chat panel with streaming, tool-call cards and approvals; a journal review agent; AI recommendations (related pages, links, tags, next actions); inline AI / Compose-with-AI surfaces; privacy and audit of everything sent.

## Acceptance Criteria
- Chat panel streams answers, shows tool calls, supports threads, cancel and approvals.
- Every agent write is user-approved, goes through the core Op queue, is audited and undoable; unanswered approvals are denied.
- Journal review and recommendations work end to end against a real Pando `agui-serve`.

## Notes
Plan: [[bitacora-v2-plan]] (decision D5).
