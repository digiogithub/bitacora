---
id: BIT-US-0007
type: story
title: Startup reconcile, cold build and incremental reindex pipeline
status: done
priority: critical
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, bitacora-index, performance]
estimate: 8
created: 2026-10-06T14:25:24Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
As a user, I want my graph to open fast and stay indexed as files change, so that search and backlinks are always current without waiting for a full reparse.

Pipeline: scan/watcher → stat filter → blake3 hash diff → parallel parse (rayon) → writer. Cold build uses the fast path (triggers dropped, batched transactions, FTS rebuild, `ANALYZE`). The watcher itself (debounce, unlink delay, echo suppression) lives in `bitacora-watch` (BIT-EP-0008); this story consumes its events.

## Acceptance Criteria
- Reopening an unchanged graph parses no file.
- Parse priority: today's journal and home page, UI-requested pages, journals newest first, pages.
- Watcher overflow or `MustScanSubDirs` triggers a full reconcile; rename events with equal hash update `files.path` and keep UUIDs.
- Rebuild-from-scratch equals incremental result on fixtures (property test).
- Cold build of a 50k-block generated graph < 5 s (benchmark in CI, non-blocking threshold report).

## Notes
Implements: BIT-SP-0003.R6, BIT-SP-0003.R7, BIT-SP-0003.R12, BIT-SP-0003.R16. See [[sqlite-index-schema]] §4.1–4.2, §4.6; [[03-parsing-indexing-search]] §1–2, §9.
