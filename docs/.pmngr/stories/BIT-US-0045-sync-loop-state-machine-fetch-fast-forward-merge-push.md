---
id: BIT-US-0045
type: story
title: "Sync loop state machine: fetch, fast-forward, merge, push, offline"
status: backlog
priority: critical
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, state-machine]
estimate: 13
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T14:28:30Z
---

## Description
As a user with several devices, I want Bitacora to fetch, integrate and push automatically and survive being offline, so that my graph converges everywhere without manual git.

## Acceptance Criteria
- Implements the state machine of [[git-sync-merge]] §2.5 (`Disabled`, `Idle`, `Dirty`, `Committing`, `Syncing`, `Fetching`, `Integrating`, `FastForward`, `Merging`, `Conflicted`, `Pushing`, `Offline`, `Error`).
- Periodic fetch 120 s foreground / 600 s background, plus network-up/wake/manual triggers.
- Fast-forward updates work tree through the core writer; divergence calls the block merge (BIT-EP-0012) and creates a two-parent merge commit via `commit-tree` + `update-ref`.
- Plain push; non-fast-forward → refetch, max 5 attempts with 1–8 s jitter → `Error(PushRejectedLoop)`.
- Offline back-off 30 s → 10 min cap, reset on network change; local commits continue.
- Never runs while a write transaction is pending; flushes editors before merging.
- Two clones syncing through a temp bare repo converge in integration tests.

## Notes
Implements: BIT-SP-0006.R4, BIT-SP-0006.R5, BIT-SP-0006.R7, BIT-SP-0006.R14. See [[git-sync-merge]] §2.2–2.5, §6. ADR-007, ADR-011.
