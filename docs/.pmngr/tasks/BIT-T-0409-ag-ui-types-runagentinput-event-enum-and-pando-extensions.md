---
id: BIT-T-0409
type: task
title: "AG-UI types: RunAgentInput, Event enum and Pando extensions"
status: backlog
priority: high
parent: BIT-US-0130
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, agui]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
Serde types for `RunAgentInput`, messages, tools, context, all event variants of `internal/agui/events.go`, JSON Patch state deltas, `StateDoc`, `pando_permission_request` payload; unknown events kept as `Event::Unknown(Value)`.

## Acceptance Criteria
- Round-trip tests against JSON fixtures captured from Pando and from the TS SDK tests.
