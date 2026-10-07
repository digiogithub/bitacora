---
id: BIT-T-0416
type: task
title: Content-hash skip on upsert and index-only documents
status: backlog
priority: high
parent: BIT-US-0132
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, kb]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
Upsert accepts `content_hash`; when it matches the stored hash only metadata/tags are updated (no re-chunk/re-embed). New `index_only` flag stores the document without writing a KB mirror file.

## Acceptance Criteria
- Go tests: unchanged hash → no embedding call; index_only → no file on disk.
