---
id: BIT-T-0432
type: task
title: Revoke consent and purge flow
status: backlog
priority: medium
parent: BIT-US-0138
milestone: BIT-M-0007
author: mcp
labels: [v2, privacy]
estimate: 1
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Revoking consent cancels outbox sending and running agent runs for the graph and offers purging its semantic documents.

## Acceptance Criteria
- Test: after revoke, no further request carries graph content.
