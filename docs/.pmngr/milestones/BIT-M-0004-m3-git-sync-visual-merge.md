---
id: BIT-M-0004
type: milestone
title: M3 — Git sync & visual merge
status: done
author: mcp
created: 2026-10-06T14:20:19Z
updated: 2026-10-07T08:21:50Z
started: 2026-10-07T00:15:15Z
closed: 2026-10-07T08:21:50Z
due: 2027-06-30
---

## Description
Automatic git sync (auto-commit, fetch, merge, push) with a block-aware 3-way merge that auto-resolves metadata and shows content conflicts in a visual per-block resolver.

## Acceptance Criteria
- Two machines editing the same graph converge without conflict markers in any file.
- Metadata-only divergences resolve without user interaction.
- Content conflicts are presented per block with ours/theirs/both/edit options.

## Notes
See [[git-sync-merge]].
