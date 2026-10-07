---
id: BIT-T-0408
type: task
title: "KbClient: upsert, delete, search, reindex"
status: backlog
priority: high
parent: BIT-US-0129
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:54:52Z
---

## Description
Implement the REST KB client for the existing routes:
- `X-Pando-Token` auth, timeouts and typed errors;
- tolerant deserialisation, so unknown fields are ignored;
- version info from `/info` when present.

## Acceptance Criteria
- Mock-server tests for each route, including 401, 409, timeout and unknown-field tolerance.
