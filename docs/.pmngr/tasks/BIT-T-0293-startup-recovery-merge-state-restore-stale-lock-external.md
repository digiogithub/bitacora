---
id: BIT-T-0293
type: task
title: "Startup recovery: merge-state restore, stale lock, external operations, markers"
status: backlog
priority: medium
parent: BIT-US-0047
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, recovery]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
`crates/bitacora-sync/src/recovery.rs` run on graph open before the first commit: (1) `.git/bitacora/merge-state.json` exists → state `Conflicted` with stored conflicts; (2) `.git/index.lock` older than 10 min and no running git process (check via `sysinfo`) → remove, log; (3) `MERGE_HEAD`/`rebase-merge`/`rebase-apply` present → `Error(ExternalOperationInProgress)` with actions `AbortExternal` (`git merge --abort`/`git rebase --abort`) or `UserWillFix`; (4) unmerged index entries or marker regions in tracked `.md` → enqueue external-conflict merge (BIT-EP-0012).

## Acceptance Criteria
- Integration tests for each case using temp repos.

## Notes
Story BIT-US-0047. Implements BIT-SP-0006.R8, BIT-SP-0006.R15. See [[git-sync-merge]] §2.6.
