---
id: BIT-T-0458
type: task
title: Approval card with diff preview applying Ops via core queue
status: in_review
priority: high
parent: BIT-US-0149
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-app, security]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T12:25:52Z
started: 2026-10-07T12:17:21Z
---

## Description
Inline card for `propose_edit` and `pando_permission_request` with block-level diff, Approve/Reject; approval applies one transaction (one undo step) and writes an audit entry in agent activity.

## Acceptance Criteria
- `#[gpui::test]`: approve → graph changed + undo restores; reject → unchanged, tool resolved as denied.
