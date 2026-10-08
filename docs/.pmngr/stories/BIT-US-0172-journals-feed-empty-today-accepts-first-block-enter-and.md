---
id: BIT-US-0172
type: story
title: "Journals feed: empty today accepts first block, Enter and indent inline"
status: done
priority: high
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bug, bitacora-app, journals]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T14:36:12Z
started: 2026-10-08T12:57:08Z
closed: 2026-10-08T14:36:12Z
---

## Description
In the journals feed, when today has no blocks the user cannot add a block and indent it; they must open the day page.

## Acceptance Criteria
- Clicking the empty today placeholder in the feed creates/focuses the first block in place.
- Enter, Tab and Shift-Tab work on it in the feed exactly as on the page view.
- The journal file is only created once the user types content (Logseq behaviour).
- View-logic test covers the empty-today path.
