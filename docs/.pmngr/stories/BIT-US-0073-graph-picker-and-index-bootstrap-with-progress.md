---
id: BIT-US-0073
type: story
title: Graph picker and index bootstrap with progress
status: in_progress
priority: high
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T18:28:35Z
started: 2026-10-06T18:28:35Z
---

## Description
As a Logseq user, I want to pick my existing graph folder (or reopen a recent one) and see indexing progress, so that I can start browsing my notes in Bitacora within seconds.

Builds on the shell from BIT-US-0025 and the index pipeline from BIT-EP-0005.

## Acceptance Criteria
- First launch shows a picker: "Open graph folder…" (native dialog) and a recent-graphs list; `--graph <path>` skips it.
- Folders without `logseq/config.edn` show a warning but can still be opened.
- While indexing, a progress indicator (files done/total) is shown in the status bar; today's journal is usable before the build finishes.
- Index rebuilt after corruption shows a toast "Index rebuilt".
- Switching graph closes the previous index cleanly.

## Notes
Implements: BIT-SP-0003.R1, BIT-SP-0003.R6 (UI side). ADR-001, ADR-005. [[gpui-and-gpui-kit]] §2.2 (Sidebar, Notification, Progress, Dialog).
