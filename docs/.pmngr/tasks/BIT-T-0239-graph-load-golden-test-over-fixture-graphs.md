---
id: BIT-T-0239
type: task
title: Graph-load golden test over fixture graphs
status: backlog
priority: medium
parent: BIT-US-0098
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, test, compat]
estimate: 2
created: 2026-10-06T14:31:40Z
updated: 2026-10-06T14:31:40Z
---

## Description
`crates/bitacora-core/tests/graph_load.rs`: for each graph under `fixtures/graphs/**` load the `Graph` and serialise a stable summary (pages sorted by key: original name, file, origin, journal day, aliases, tags, namespace parent, diagnostics) to `fixtures/graphs/<name>/expected/graph.json`; compare with `insta` snapshots. Include a legacy-format graph (`fixtures/graphs/legacy/` with `Version 1.0.md`, `Projects%2FBitacora.md`, a `title::` page) and a triple-lowbar graph.

## Acceptance Criteria
- Snapshots committed and reviewed; CI fails on drift.
- Test asserts no file under the fixture changed (hash before/after).

## Notes
Refs BIT-SP-0002.R1, R5, R7, R8, R12, R18.
