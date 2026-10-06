---
id: BIT-US-0047
type: story
title: Sync status indicator, Sync now and startup recovery
status: in_progress
priority: medium
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, ui, recovery]
estimate: 5
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T19:51:49Z
started: 2026-10-06T19:51:49Z
---

## Description
As a user, I want to see at a glance whether my graph is synced and have Bitacora recover cleanly from crashes or external git operations, so that I trust the sync and never lose work.

## Acceptance Criteria
- `SyncStatus` (state, ahead/behind, last sync, conflict count, last error, active backend) published on a watch channel; status bar shows icon + "N local commits not synced" / "Conflicts (N)" / error with Retry.
- The sync popover and settings show the active git backend (system git path + version, or built-in gix) and, on auth failure with gix, suggest installing git (ADR-020).
- "Sync now" command (palette + status bar) triggers `commit → sync`.
- Startup recovery: restore `Conflicted` from `merge-state.json`; remove stale `.git/index.lock` (>10 min, no git process); detect external rebase/merge in progress → `Error(ExternalOperationInProgress)` with abort/fix choice; detect unmerged entries / marker regions and hand them to the merge.
- Settings page exposes `sync.*` settings of [[git-sync-merge]] §7.

## Notes
Implements: BIT-SP-0006.R5, BIT-SP-0006.R6, BIT-SP-0006.R8, BIT-SP-0006.R15. See [[git-sync-merge]] §2.6, §6, §7. ADR-020.
