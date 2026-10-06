---
id: BIT-T-0366
type: task
title: External conflict markers and unmerged index entries import; marker guard
status: backlog
priority: critical
parent: BIT-US-0053
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, bitacora-index, merge, conflicts]
estimate: 2
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T14:34:50Z
---

## Description
`crates/bitacora-sync/src/merge/external.rs`: parse well-formed marker regions (`<<<<<<< `, optional `||||||| `, `=======`, `>>>>>>> `) into ours/base/theirs texts (base = common text when no diff3 section) and run `merge_page`; malformed → `Conflict::ExternalMarkers` showing raw file. Unmerged index entries (stages 1/2/3 from gix status) → same merge, then `git add` the clean result. Marker guard: `assert_no_markers(output)` called in emit and before every merge write. `bitacora-index`: skip lines matching marker regex when indexing blocks of a file flagged as conflicted.

## Acceptance Criteria
- Tests for the three scenarios of BIT-SP-0006.R8; indexing test shows no marker text in FTS.

## Notes
Story BIT-US-0053. Implements BIT-SP-0006.R8. ADR-008. See [[git-sync-merge]] §5.4.
