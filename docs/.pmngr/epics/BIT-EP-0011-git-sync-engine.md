---
id: BIT-EP-0011
type: epic
title: Git sync engine
status: in_review
priority: high
milestone: BIT-M-0004
author: mcp
labels: [git, sync]
created: 2026-10-06T14:21:13Z
updated: 2026-10-07T00:15:15Z
started: 2026-10-07T00:15:15Z
---

## Description
`bitacora-sync`: `GitBackend` trait with `CliBackend` (fetch, push, commit, clone with the user's git auth, used when a system git ≥ 2.38 is installed; git is never bundled) and `GixBackend` (status, blobs, merge-base, tree diff, building merged trees; plus fetch/push/clone/commit/ref updates as a full pure-Rust fallback when no system git is found). Backend detection at startup with a UI indicator of the active backend. Sync loop: idle-debounced auto-commit, periodic fetch, merge, push with retry and backoff, offline mode, status indicator, `.gitignore` defaults, `.git/info/attributes` with `*.md merge=binary`, askpass bridge to UI (CLI) / keyring + in-app prompt (gix), clone/init graph from remote.

## Acceptance Criteria
- Two clones syncing through a bare repo converge in integration tests, run against both backends (CLI hybrid and gix-only).
- Works with SSH (agent, `~/.ssh/config`) and HTTPS credential helpers on 3 OSes when system git is installed; works over HTTPS (keyring) and SSH (gix transport) without git installed.
- Sync never runs while a write transaction is pending.

## Notes
ADR-007, ADR-020 (git not bundled; system git if installed, else gix for everything). See [[git-sync-merge]] §2–3.
