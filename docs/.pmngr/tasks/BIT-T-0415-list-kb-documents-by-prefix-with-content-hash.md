---
id: BIT-T-0415
type: task
title: List KB documents by prefix with content hash
status: backlog
priority: high
parent: BIT-US-0132
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, kb]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
`GET /api/v1/remembrances/kb/documents?path_prefix=&cursor=` returning `{file_path, content_hash, updated_at}` pages; store a content hash per document if not already stored.

## Acceptance Criteria
- Go tests for pagination and prefix exactness.
