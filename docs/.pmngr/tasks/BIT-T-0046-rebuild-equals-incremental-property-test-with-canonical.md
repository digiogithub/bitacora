---
id: BIT-T-0046
type: task
title: Rebuild-equals-incremental property test with canonical index dump
status: done
priority: critical
parent: BIT-US-0007
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, testing]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
`crates/bitacora-index/src/testing/dump.rs`: `canonical_dump(conn) -> String` rendering pages (by name), files (by path, without mtime/indexed_at), blocks keyed by `(path, ord)` (without rowid; UUID only when `uuid_source = 1`), refs, properties, aliases, tags, diagnostics, and FTS hits for a fixed probe-term list, all sorted.
`crates/bitacora-index/tests/rebuild_equals_incremental.rs`: `proptest` generating sequences of 10–50 operations on a copy of a fixture graph (edit block text, add/remove `[[ref]]`, add/remove property, change `title::`, add `alias::`, rename file, delete file, create file). Apply incrementally through the pipeline, then cold-build a second index on the final state; assert dumps equal.

## Acceptance Criteria
- Test runs in CI with at least 64 cases per fixture graph; failures shrink to a minimal op sequence.
- Documented in AGENTS.md §6 expectation for `bitacora-index`.

## Notes
BIT-SP-0003.R16, BIT-SP-0003.R5.
