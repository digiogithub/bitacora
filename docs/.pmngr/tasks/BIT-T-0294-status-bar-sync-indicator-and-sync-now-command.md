---
id: BIT-T-0294
type: task
title: Status bar sync indicator and Sync now command
status: in_progress
priority: medium
parent: BIT-US-0047
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, sync, ui]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T19:59:19Z
started: 2026-10-06T19:59:19Z
---

## Description
`crates/bitacora-app/src/views/status_bar/sync.rs`: subscribe to `watch::Receiver<SyncStatus>` via the tokio bridge; icon + text per state (Synced 2 min ago / Syncing… / Offline · 3 local commits not synced / Conflicts (N) / Error: … [Retry]); tooltip with ahead/behind and last error; click opens sync popover with "Sync now". Command palette action `sync: sync now` sends `SyncCommand::SyncNow`.

## Acceptance Criteria
- `#[gpui::test]` rendering each state string from a stub status.

## Notes
Story BIT-US-0047. Implements BIT-SP-0006.R5.
