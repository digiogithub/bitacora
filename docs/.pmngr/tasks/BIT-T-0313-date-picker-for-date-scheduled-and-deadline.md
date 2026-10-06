---
id: BIT-T-0313
type: task
title: Date picker for /date, SCHEDULED and DEADLINE
status: backlog
priority: medium
parent: BIT-US-0105
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, editor]
estimate: 2
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
GPUI Kit `DatePicker`/`Calendar` + `TimeField` popover anchored at the caret for `/date` (inserts `[[<journal title>]]` with configured format), `/scheduled` and `/deadline` (writes `SCHEDULED: <2026-10-06 Tue>` / with time and repeater options `.+1d`, `++1w`, `+1m`) on the line after the block title, matching Logseq formatting.

## Acceptance Criteria
- Golden tests: output strings identical to Logseq for 6 cases (date only, with time, each repeater kind).

## Notes
[[04-editor-outliner-operations]] Requirements 15; [[gpui-and-gpui-kit]] §2.2 (Calendar, DatePicker, TimeField).
