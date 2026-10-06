---
id: BIT-T-0190
type: task
title: Virtual today's journal and midnight rollover
status: done
priority: high
parent: BIT-US-0076
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, journals]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:08:15Z
closed: 2026-10-06T19:08:15Z
---

## Description
In `JournalsView`, prepend today's journal computed with `bitacora-core` journal naming (title format from config, file name `journals/yyyy_MM_dd.md`). If no file/page exists, render a virtual empty page (no file is created; the editor epic creates it on first edit). A 5 s timer (and window focus) checks the date and inserts the new today entry after midnight.

## Acceptance Criteria
- `#[gpui::test]` with an injected clock: rollover inserts the new day at top.
- Graph folder unchanged after viewing today's virtual journal (file hash check).

## Notes
[[block-editor]] §8; [[04-editor-outliner-operations]] §9 (`create-today-journal!`).
