---
id: BIT-T-0490
type: task
title: Auto-open most recent graph at startup with fallback to picker
status: done
priority: high
parent: BIT-US-0165
milestone: BIT-M-0006
author: mcp
labels: [v2, ux, bitacora-app]
estimate: 2
created: 2026-10-07T09:55:34Z
updated: 2026-10-07T10:21:58Z
started: 2026-10-07T10:21:50Z
closed: 2026-10-07T10:21:58Z
---

## Description
In `open_workspace`, when `args.graph` is `None` and "Reopen last graph" is on, take the first existing entry from `RecentGraphs` and call `open_graph`. If it is missing, show the picker with a notice. Add the setting, default on.

## Acceptance Criteria
- `#[gpui::test]` covers a stored recent, a missing path, and the setting off.
