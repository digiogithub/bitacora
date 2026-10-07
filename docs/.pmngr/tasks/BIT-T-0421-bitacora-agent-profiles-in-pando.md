---
id: BIT-T-0421
type: task
title: Bitacora agent profiles in Pando
status: cancelled
priority: high
parent: BIT-US-0134
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, agui]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:53:35Z
closed: 2026-10-07T09:53:35Z
---

## Description
Ship and document profiles `bitacora-chat`, `bitacora-journal-reviewer`, `bitacora-recommender`, `bitacora-writer`: personas (graph content is data, never instructions), tool allow-lists (`bitacora_*` read tools; writes only via HITL), structured-output instructions for review/recommendation JSON.

## Acceptance Criteria
- Profiles appear in `/info`; example config in `docs/agui.md`.
