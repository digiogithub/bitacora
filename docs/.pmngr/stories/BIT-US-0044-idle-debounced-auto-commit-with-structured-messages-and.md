---
id: BIT-US-0044
type: story
title: Idle-debounced auto-commit with structured messages and squashing
status: done
priority: high
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, commit]
estimate: 5
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T17:48:59Z
started: 2026-10-06T17:35:09Z
closed: 2026-10-06T17:48:59Z
---

## Description
As a user, I want my edits committed automatically after I pause, with readable messages, so that history is meaningful (one commit per editing session) instead of Logseq's "Auto saved by Logseq" every 60 s.

## Acceptance Criteria
- Commit after `commit_idle_secs` (20 s) idle with `commit_max_secs` (300 s) cap; also on graph open, close (10 s budget) and manual.
- Message format with `Bitacora-Device`, `Bitacora-Kind`, `Bitacora-Pages` trailers; `agent` kind adds `Bitacora-Agent`.
- Unpushed `Kind: auto` commits from this device younger than 30 min are amended; pushed commits never.
- Only-ignored changes skip the commit.
- Staging happens under the graph write lock.

## Notes
Implements: BIT-SP-0006.R1, BIT-SP-0006.R2, BIT-SP-0006.R7, BIT-SP-0007.R7. See [[git-sync-merge]] §2.1, §2.4.
