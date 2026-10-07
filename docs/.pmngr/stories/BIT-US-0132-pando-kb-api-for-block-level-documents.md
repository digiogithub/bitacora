---
id: BIT-US-0132
type: story
title: Pando KB API for block-level documents
status: cancelled
priority: high
parent: BIT-EP-0019
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, kb, api]
estimate: 8
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T09:53:36Z
closed: 2026-10-07T09:53:36Z
---

## Description
As Bitacora, I want Pando's KB API to handle many small documents efficiently, so that block-level semantic sync is cheap and reconcilable.

## Acceptance Criteria
- `GET /api/v1/remembrances/kb/documents?path_prefix=` lists `{file_path, content_hash, updated_at}` (paginated).
- Upsert accepts optional `content_hash` and skips re-chunking/re-embedding when unchanged (metadata still updated).
- `POST .../kb/documents/batch` `{upserts[], deletes[]}` with per-item results.
- `DELETE .../kb/documents?path_prefix=` deletes a whole prefix.
- `index_only` flag: no mirror file written for such documents.
- Go tests and API docs for each.

## Notes
Pando files: `internal/api/routes.go`, `handlers_remembrances_kb.go`, KB store `kb.go` (~941 path_prefix SQL).
