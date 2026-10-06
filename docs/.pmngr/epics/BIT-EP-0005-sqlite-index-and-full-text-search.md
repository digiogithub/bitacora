---
id: BIT-EP-0005
type: epic
title: SQLite index and full-text search
status: backlog
priority: critical
milestone: BIT-M-0002
author: mcp
labels: [index, search]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
`bitacora-index`: schema and migrations from [[sqlite-index-schema]], index stored in the platform data dir (ADR-005), incremental pipeline (size/mtime → blake3 hash → parallel parse → single-writer transactional replace per file), placeholder pages, path-refs via range joins, backlinks, aliases, tasks view, FTS5 (word + trigram) search with ranking and fuzzy page-title matching, full rebuild + integrity check.

## Acceptance Criteria
- Full rebuild result equals incremental result on the fixture graphs.
- Search returns results for a 5,000-page graph in < 50 ms p95.
- Index can be deleted at any time and is rebuilt automatically.

## Notes
ADR-004, ADR-005. See [[03-parsing-indexing-search]].
