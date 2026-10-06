---
id: BIT-US-0070
type: story
title: "\"Page changed on disk\" conflict notice with keep mine / take disk / show diff"
status: backlog
priority: high
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [ui, io, bitacora-app, bitacora-core]
estimate: 5
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T14:29:01Z
---

## Description
As a user whose page was edited both in Bitacora and elsewhere, I want a clear non-modal notice that lets me keep my version, take the disk version or see a diff, so that I decide what wins and nothing is lost either way.

## Acceptance Criteria
- Core `PageState::Conflicted { disk: DiskSnapshot }` stops writes for that page; other pages keep saving.
- Non-modal banner on the page with [Keep mine (overwrite)], [Take disk version], [Show diff].
- Keep mine: back up disk version to `logseq/bak`, write ours (with fresh hash check).
- Take disk: back up ours, reload disk version, push an undoable transaction to restore ours.
- Show diff: block-level side-by-side diff of disk vs ours.
- MCP writes to a conflicted page are refused with "page has an unresolved on-disk conflict".

## Notes
Implements: BIT-SP-0005.R16.
See [[block-editor]] §6.2, §9 item 14; [[04-editor-outliner-operations]] §5 steps 7–8 (Logseq diff modal); [[mcp-server]]. ADR-011.
