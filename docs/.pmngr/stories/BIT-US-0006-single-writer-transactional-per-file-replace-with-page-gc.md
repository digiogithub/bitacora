---
id: BIT-US-0006
type: story
title: Single-writer transactional per-file replace with page GC and UUID carry-over
status: backlog
priority: critical
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, bitacora-index]
estimate: 8
created: 2026-10-06T14:25:24Z
updated: 2026-10-06T14:25:24Z
---

## Description
As a developer, I want one writer thread that replaces everything derived from a file in a single transaction, upserts pages, garbage-collects placeholders and keeps block UUIDs stable, so that readers always see a consistent index and block identity survives external edits.

## Acceptance Criteria
- Steps 0–11 of [[sqlite-index-schema]] §4.4 are implemented; `delete_file(path)` demotes the page and runs GC.
- Built-in pages are seeded and never GC'ed.
- UUID precedence: `id::` > carried over by diff > UUIDv7; duplicates across files produce a diagnostic and a fresh UUID.
- `IndexEvent::FileReplaced` is emitted after each commit.
- A per-file replace of a 500-block page takes < 10 ms (benchmark).

## Notes
Implements: BIT-SP-0003.R5, BIT-SP-0003.R7, BIT-SP-0003.R11, BIT-SP-0003.R12. ADR-004, ADR-006. See [[sqlite-index-schema]] §2, §4.4.
