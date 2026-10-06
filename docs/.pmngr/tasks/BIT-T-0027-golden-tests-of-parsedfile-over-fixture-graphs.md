---
id: BIT-T-0027
type: task
title: Golden tests of ParsedFile over fixture graphs
status: done
priority: high
parent: BIT-US-0005
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, testing]
estimate: 2
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T17:32:45Z
closed: 2026-10-06T17:32:45Z
---

## Description
`crates/bitacora-index/tests/parsed_golden.rs`: for every `.md` under `fixtures/graphs/**`, serialize `ParsedFile` (minus generated UUIDs) to pretty JSON and compare with `fixtures/expected/index/<graph>/<path>.json` using `insta` snapshots. Where possible, expected refs/properties are cross-checked against values exported from Logseq 0.10.15 (document the export script in `fixtures/README.md`).

## Acceptance Criteria
- Snapshots committed for all fixture graphs; `cargo test -p bitacora-index` passes.
- At least 10 hand-verified cases cover namespaces, property refs, markers, embeds and journals.

## Notes
BIT-SP-0003.R8, BIT-SP-0003.R10. AGENTS.md §6.
