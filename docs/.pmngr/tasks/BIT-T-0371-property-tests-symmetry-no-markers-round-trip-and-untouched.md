---
id: BIT-T-0371
type: task
title: "Property tests: symmetry, no markers, round-trip and untouched-bytes over corpus"
status: backlog
priority: high
parent: BIT-US-0055
milestone: BIT-M-0004
author: mcp
labels: [bitacora-merge, merge, testing]
estimate: 3
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T15:10:04Z
---

## Description
`crates/bitacora-merge/tests/merge_props.rs` with `proptest`: for each page in `fixtures/graphs/**`, generate two random edit scripts (block text edits, inserts, deletes, moves, property/metadata toggles) → ours/theirs; assert: output never contains marker lines; output parses and round-trips; blocks untouched by both sides are byte-identical to ours; swapping ours/theirs yields the same block multiset and same conflict set (ignoring ours-first ordering and collapsed policy); merging with base == ours returns theirs byte-for-byte.

## Acceptance Criteria
- 256 cases default (configurable); runs < 60 s; shrinking produces minimal repros.

## Notes
Story BIT-US-0055. Verifies BIT-SP-0006.R8, BIT-SP-0006.R12, BIT-SP-0006.R14.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
