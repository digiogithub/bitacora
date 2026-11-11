---
id: BIT-US-0167
type: story
title: Editable SCHEDULED/DEADLINE date chips with a calendar picker
status: done
priority: medium
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ux, editor, bitacora-app, bitacora-markdown]
created: 2026-10-07T17:31:17Z
updated: 2026-11-11T20:31:27Z
started: 2026-10-07T17:31:17Z
closed: 2026-11-11T20:31:27Z
---

## Description
Clicking a SCHEDULED/DEADLINE chip opens a calendar popover (PopoverShell anchored to the chip) to pick another date or remove it. The planning line is rewritten in Logseq 0.10.x format touching only the date bytes (repeaters/times kept), through the core command queue (single undo step). Works in the page outline (block_view) and the Tasks view; references get it when their rows carry RowEdit. `/scheduled` and `/deadline` slash commands (Logseq: clear slash, show date picker) already exist and are covered by tests.

## Acceptance Criteria
- Chip click opens the picker; picking rewrites only `YYYY-MM-DD Ddd`.
- "Remove date" deletes the entry/line.
- Undo restores exact bytes.
- Slash commands insert/replace the line right under the first line.
