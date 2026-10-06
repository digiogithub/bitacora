---
id: BIT-T-0285
type: task
title: Squash unpushed auto commits from the same device
status: done
priority: medium
parent: BIT-US-0044
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, commit]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:48:56Z
closed: 2026-10-06T17:48:56Z
---

## Description
In `autocommit.rs`: before committing `Kind: auto`, inspect HEAD (gix): parse trailers; if `Kind: auto`, same `Bitacora-Device`, HEAD not reachable from `refs/remotes/origin/<branch>`, and commit time < 30 min ago and `sync.squash_auto_commits` → `git commit --amend` with merged `Bitacora-Pages` set and recomputed subject. Never amend merge/resolve/agent/manual commits.

## Acceptance Criteria
- Integration tests (temp repo + bare remote): amend within window; new commit after push; new commit after 30 min (fake clock via `GIT_COMMITTER_DATE`).

## Notes
Story BIT-US-0044. Implements BIT-SP-0006.R2.
