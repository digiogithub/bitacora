---
id: BIT-EP-0011
type: epic
title: Git sync engine
status: backlog
priority: high
milestone: BIT-M-0004
author: mcp
labels: [git, sync]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
`bitacora-sync`: `GitBackend` trait with `CliBackend` (fetch, push, commit, clone with the user's git auth; MinGit bundled on Windows) and `GixBackend` (status, blobs, merge-base, tree diff, building merged trees). Sync loop: idle-debounced auto-commit, periodic fetch, merge, push with retry and backoff, offline mode, status indicator, `.gitignore` defaults, `.git/info/attributes` with `*.md merge=binary`, askpass bridge to UI, clone/init graph from remote.

## Acceptance Criteria
- Two clones syncing through a bare repo converge in integration tests.
- Works with SSH (agent, `~/.ssh/config`) and HTTPS credential helpers on 3 OSes.
- Sync never runs while a write transaction is pending.

## Notes
ADR-007. See [[git-sync-merge]] §2–3.
