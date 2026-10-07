---
id: BIT-T-0417
type: task
title: Batch upsert/delete and delete-by-prefix endpoints
status: cancelled
priority: high
parent: BIT-US-0132
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, kb]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:53:35Z
closed: 2026-10-07T09:53:35Z
---

## Description
`POST .../kb/documents/batch` `{upserts[], deletes[]}` with per-item status and a size limit; `DELETE .../kb/documents?path_prefix=` removing all matching documents and chunks.

## Acceptance Criteria
- Go tests incl. partial failures and large batches; API docs updated.
