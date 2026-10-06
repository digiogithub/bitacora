---
id: BIT-T-0365
type: task
title: Block-level rerere memo and merge recompute when the remote moves
status: backlog
priority: medium
parent: BIT-US-0053
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge, conflicts]
estimate: 2
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T14:34:50Z
---

## Description
`crates/bitacora-sync/src/merge/memo.rs`: `ResolutionMemo` keyed by `(path, block_key, hash(base), hash(ours), hash(theirs))` stored inside merge-state; on new fetch while `Conflicted`, recompute `sync_merge` with the same base and new remote (and new local HEAD), then auto-apply memo entries whose key still matches; changed theirs hash → conflict reappears unresolved.

## Acceptance Criteria
- Tests for both scenarios of BIT-SP-0006.R21.

## Notes
Story BIT-US-0053. Implements BIT-SP-0006.R21.
