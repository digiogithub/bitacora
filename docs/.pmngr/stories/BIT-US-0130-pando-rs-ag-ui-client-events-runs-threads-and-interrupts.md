---
id: BIT-US-0130
type: story
title: "pando-rs AG-UI client: events, runs, threads and interrupts"
status: backlog
priority: high
parent: BIT-EP-0018
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, rust, agui]
estimate: 8
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T09:14:29Z
---

## Description
As a Rust app, I want an AG-UI client that streams agent runs from Pando, so that I can embed chat and agent features natively.

## Acceptance Criteria
- Typed `RunAgentInput` and an `Event` enum covering all AG-UI events emitted by Pando (`internal/agui/events.go:10-41`: RUN_*, STEP_*, TEXT_MESSAGE_*, TOOL_CALL_*, STATE_SNAPSHOT/DELTA (RFC 6902), MESSAGES_SNAPSHOT, ACTIVITY_*, REASONING_*, RAW, CUSTOM) plus Pando extensions (`StateDoc`, `pando_permission_request`); unknown events preserved, not fatal.
- SSE run stream (`POST /{agent}`), `/info`, `/healthz`, threads (`list`, `messages`, `stream` re-attach, `delete`), `POST /runs/{id}/cancel`, bearer auth.
- Frontend tools: declare tools, receive interrupt (`RUN_FINISHED{outcome:"interrupt"}`), resume with tool result; HITL helper to answer permission requests.

## Notes
Port shape from `sdk/typescript/src/agui/` and `sdk/python/src/pando/agui/client.py`. Evaluate `ag-ui-core` as type source (record decision).
