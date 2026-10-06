---
id: BIT-T-0375
type: task
title: Active git backend indicator and install-git suggestion
status: done
priority: medium
parent: BIT-US-0047
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, sync, ui]
estimate: 1
created: 2026-10-06T15:15:10Z
updated: 2026-10-06T20:37:17Z
started: 2026-10-06T19:59:19Z
closed: 2026-10-06T20:37:17Z
---

## Description
Add `backend: ActiveBackend { SystemGit { path, version }, Gix { reason: NotFound | TooOld(version) } }` to `SyncStatus`. In the sync popover / tooltip of the status bar (BIT-T-0294) and on the sync settings page (BIT-T-0295) show "Git: system git 2.47 (/usr/bin/git)" or "Git: built-in (gix) — system git not found". When an auth error occurs on the gix backend, the error banner includes "Install git to use your SSH config and credential helpers" with a link to the git download page; `bitacora-cli doctor` prints the active backend.

## Acceptance Criteria
- `#[gpui::test]` renders both backend variants and the auth-failure hint from a stub status.
- `bitacora-cli doctor` output includes the backend line.

## Notes
Story BIT-US-0047. Implements BIT-SP-0006.R6. ADR-020.
