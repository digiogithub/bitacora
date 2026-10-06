---
id: BIT-T-0372
type: task
title: "End-to-end conflict test: two clones, bare remote, resolve and converge"
status: backlog
priority: high
parent: BIT-US-0055
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, testing]
estimate: 2
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T14:34:50Z
---

## Description
`crates/bitacora-sync/tests/e2e_conflict.rs`: clones A and B of a temp bare repo; same block edited differently + a metadata-only change + rename on one side + asset collision; B syncs first, A syncs → `Conflicted` with exactly 1 content conflict (others auto-resolved), work tree holds ours, no push; resolve via `MergeState::resolve(Theirs)` → resolve commit pushed; B fetches → both trees identical; also simulate `git pull` run by the user in A (binary driver) and verify Bitacora cleans it.

## Acceptance Criteria
- Deterministic, < 15 s, 3-OS CI.

## Notes
Story BIT-US-0055. Verifies BIT-SP-0006.R8, BIT-SP-0006.R15, BIT-SP-0006.R17, BIT-SP-0006.R19.
