---
id: BIT-US-0076
type: story
title: Journals view with infinite scroll and virtual today's journal
status: backlog
priority: high
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, journals, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T14:29:26Z
---

## Description
As a daily-notes user, I want the Journals view to show today's journal on top followed by past journals as I scroll, so that I can review recent days the same way as in Logseq.

## Acceptance Criteria
- Newest-first list of journal pages from `pages.journal_day`; loads 7 more on scroll.
- Today's journal appears even if its file does not exist (virtual page, no file created).
- Day rollover at midnight inserts the new today entry (checked every 5 s or on window focus).
- Non-today journals render their blocks only when scrolled into view.
- Clicking a journal title opens its page view.

## Notes
[[04-editor-outliner-operations]] §9; [[block-editor]] §8 (virtual today's journal). Journal naming rules in BIT-SP-0002.
