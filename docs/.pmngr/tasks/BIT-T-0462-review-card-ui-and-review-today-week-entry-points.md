---
id: BIT-T-0462
type: task
title: Review card UI and Review today/week entry points
status: backlog
priority: medium
parent: BIT-US-0151
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, journal, bitacora-app]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T09:19:06Z
---

## Description
Buttons in journal header and Agent tab; amber review card with sections; tasks clickable; "Insert into journal" triggers `propose_edit` approval.

## Acceptance Criteria
- `#[gpui::test]`; no graph change without approval.
