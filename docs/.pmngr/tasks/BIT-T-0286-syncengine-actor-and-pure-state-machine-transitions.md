---
id: BIT-T-0286
type: task
title: SyncEngine actor and pure state machine transitions
status: backlog
priority: critical
parent: BIT-US-0045
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, state-machine]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
`crates/bitacora-sync/src/engine/state.rs`: `enum SyncState { Disabled, Idle, Dirty, Committing, Syncing, Fetching, Integrating, FastForward, Merging, Conflicted, Pushing{attempt}, Offline{retry_at, backoff_idx}, Error(SyncErrorKind) }` and `fn transition(state, event) -> (SyncState, Vec<Effect>)` (pure, table-driven per [[git-sync-merge]] §2.5). Events: `FileChanged`, `IdleTimeout`, `Cap`, `Tick`, `NetworkUp`, `Manual`, `FetchDone{outcome}`, `MergeDone{conflicts}`, `PushDone{outcome}`, `AllResolved`, `Disable`, `Retry`. `engine/actor.rs`: one tokio task per graph owning `Box<dyn GitBackend>`, command `mpsc` channel (`SyncNow`, `Shutdown{budget}`), status `watch::Sender<SyncStatus>`; executes effects.

## Acceptance Criteria
- Exhaustive unit tests of the transition table (every arrow in §2.5 diagram).
- Status published on every transition.

## Notes
Story BIT-US-0045. Implements BIT-SP-0006.R4, BIT-SP-0006.R5.
