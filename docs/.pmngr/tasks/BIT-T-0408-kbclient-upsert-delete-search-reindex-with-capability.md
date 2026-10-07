---
id: BIT-T-0408
type: task
title: "KbClient: upsert, delete, search, reindex with capability detection"
status: backlog
priority: high
parent: BIT-US-0129
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
Implement the REST KB client with `X-Pando-Token`, timeouts, typed errors, tolerant deserialisation (unknown fields ignored), and capability detection from `/info` for batch/list/hash-skip/delete-prefix/index-only.

## Acceptance Criteria
- Mock-server tests for each route incl. 401, 409, timeout, unknown-field tolerance.
