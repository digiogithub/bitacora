---
id: BIT-T-0411
type: task
title: Threads API and re-attach to running streams
status: backlog
priority: medium
parent: BIT-US-0130
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, agui]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
`threads()` (paginated), `thread_messages(id)`, `attach(id)` via `GET /threads/{id}/stream` with `MESSAGES_SNAPSHOT` resync, `delete_thread(id)`.

## Acceptance Criteria
- Mock-server tests; re-attach yields snapshot then live events.
