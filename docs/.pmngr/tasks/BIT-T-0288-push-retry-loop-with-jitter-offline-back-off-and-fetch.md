---
id: BIT-T-0288
type: task
title: Push retry loop with jitter, offline back-off and fetch scheduling
status: done
priority: high
parent: BIT-US-0045
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, state-machine, network]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:48:56Z
closed: 2026-10-06T17:48:56Z
---

## Description
`crates/bitacora-sync/src/engine/schedule.rs`: periodic fetch 120 s (foreground) / 600 s (background, set via `SyncCommand::SetForeground(bool)`); push non-fast-forward → refetch + integrate, max 5 attempts with uniform 1–8 s jitter → `Error(PushRejectedLoop)`; network errors → `Offline` with back-off [30 s, 60 s, 120 s, 300 s, 600 s]; `NetworkUp`/wake events (from app, e.g. `netwatcher`/OS hooks or periodic DNS probe) reset back-off and sync immediately; close: commit + best-effort push within 10 s budget.

## Acceptance Criteria
- Fake-clock tests for back-off sequence, reset, 5-attempt limit, close budget.

## Notes
Story BIT-US-0045. Implements BIT-SP-0006.R4, BIT-SP-0006.R5.
