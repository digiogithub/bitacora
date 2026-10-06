---
id: BIT-T-0211
type: task
title: "\"Agent activity\" view with filters and one-click undo"
status: done
priority: medium
parent: BIT-US-0022
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, audit, ui]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T21:54:02Z
closed: 2026-10-06T21:54:02Z
---

## Description
`crates/bitacora-app/src/views/agent_activity.rs`: GPUI Kit list of audit entries (tail-reading the JSONL, newest first, virtualized), filter by token/tool/result, entry detail (summary, affected blocks with jump-to), Undo button calling `revert_transaction` (disabled with tooltip when `can_revert` is false), "undone" badge. Toast "edited by agent" on pages links here.

## Acceptance Criteria
- `#[gpui::test]` for filter and undo-enabled state logic.

## Notes
Story BIT-US-0022. Implements BIT-SP-0007.R8. ADR-001.
