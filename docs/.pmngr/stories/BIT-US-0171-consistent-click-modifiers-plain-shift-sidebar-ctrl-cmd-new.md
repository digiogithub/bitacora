---
id: BIT-US-0171
type: story
title: "Consistent click modifiers: plain, Shift (sidebar), Ctrl/Cmd (new tab)"
status: in_review
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-app]
created: 2026-10-07T19:26:03Z
updated: 2026-10-07T19:27:46Z
started: 2026-10-07T19:26:03Z
---

## Description
Every place a page or block can be opened honours the same modifiers via one helper (OpenIn::from_modifiers): click = current behaviour, Shift = right sidebar, Ctrl/Cmd = new tab.

## Acceptance Criteria
- Refs, tags, block refs, bullets, journal titles, favorites/recents, references, query results, tasks, all pages, graph nodes, palette results.
- gpui tests per surface; docs/user/ui-tour updated.
