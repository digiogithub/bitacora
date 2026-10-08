---
id: BIT-US-0178
type: story
title: Today's journal shows scheduled-today, tomorrow and DOING tasks below its blocks
status: done
priority: medium
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-app, journals, tasks]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T14:36:13Z
started: 2026-10-08T12:57:16Z
closed: 2026-10-08T14:36:13Z
---

## Description
Like Logseq's default queries: under today's journal blocks show sections "Scheduled today", "Tomorrow" and "Doing" with the matching tasks (clickable, live-updating).

## Acceptance Criteria
- Only on today's journal (feed and page view).
- Sections hide when empty; task items navigate to the block and can toggle the marker.
- Reuses the Tasks view queries; refreshes on index updates.
