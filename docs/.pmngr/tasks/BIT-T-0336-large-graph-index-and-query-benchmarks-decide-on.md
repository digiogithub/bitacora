---
id: BIT-T-0336
type: task
title: Large-graph index and query benchmarks; decide on materialized path-refs
status: in_progress
priority: medium
parent: BIT-US-0109
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, benchmark, performance]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T22:47:18Z
started: 2026-10-06T22:47:18Z
---

## Description
Extend `crates/bitacora-index/benches/` with the `large` preset (~500k blocks): cold build, search p95, page outline open, linked references of the most-referenced page, and DSL queries `(and [[a]] [[b]] [[c]])`, `(task TODO DOING)`, `(between -30d today)`. If the path-refs view misses targets, prototype `block_path_refs_mat(page_id, block_id)` computed per file in `ParsedFile` (file-local, replaced with the file) behind a schema bump, and compare. Record results and the decision as a new ADR row in `docs/architecture.md` and update [[sqlite-index-schema]] §3.1.

## Acceptance Criteria
- Targets: cold build < 60 s, search p95 < 100 ms, linked refs < 100 ms, 3-ref AND query < 200 ms; numbers published in the bench job summary.
- Decision documented (ADR) with data.

## Notes
BIT-SP-0003.R14, BIT-SP-0003.R16; [[sqlite-index-schema]] §9 item 17 (MAY materialize).
