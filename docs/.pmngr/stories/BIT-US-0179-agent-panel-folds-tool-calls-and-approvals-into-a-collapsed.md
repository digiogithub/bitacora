---
id: BIT-US-0179
type: story
title: Agent panel folds tool calls and approvals into a collapsed block
status: done
priority: medium
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-app, ai, agent]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T14:36:13Z
started: 2026-10-08T12:57:16Z
closed: 2026-10-08T14:36:13Z
---

## Description
Tool calls and approvals flood the transcript before the answer. Group consecutive tool calls/approvals of a turn into one collapsed "N tool calls" block, expandable on click.

## Acceptance Criteria
- Collapsed by default; header shows count and running/failed state.
- Pending approvals that need user action remain visible (auto-expanded) until resolved, then fold.
- Assistant text answer always visible.
