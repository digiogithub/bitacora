---
id: BIT-US-0048
type: story
title: Per-page history with block-level diff and selective restore
status: in_progress
priority: low
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, history, ui]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T19:51:49Z
started: 2026-10-06T19:51:49Z
---

## Description
As a user, I want to browse the git history of a page, see block-level differences and restore a single block or the whole page, so that I can recover lost text without touching git (Logseq only offers raw `git log -p` and whole-file revert).

## Acceptance Criteria
- Page menu "History" lists commits touching the page path (following renames) with device, kind and time from trailers.
- Selecting a version shows a block-level diff vs current (added/removed/changed; metadata-only changes hidden by default).
- "Restore block" and "Restore page" apply as undoable core ops.

## Notes
Implements: BIT-SP-0006.R22. See [[git-sync-merge]] Requirements, [[05-git-and-apis]] §1.4, [[block-editor]].
