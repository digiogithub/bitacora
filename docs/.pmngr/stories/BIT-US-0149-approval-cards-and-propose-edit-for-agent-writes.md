---
id: BIT-US-0149
type: story
title: Approval cards and propose_edit for agent writes
status: backlog
priority: high
parent: BIT-EP-0022
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, agui, security, bitacora-app, bitacora-core]
estimate: 8
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T09:18:16Z
---

## Description
As a user, I want any change an agent proposes to my notes to be shown as a diff that I approve or reject, applied safely and undoable.

## Acceptance Criteria
- Frontend tools declared on every run: `propose_edit(ops)`, `open_page(name)`, `get_selection()`; schema-validated arguments.
- `propose_edit` and `pando_permission_request` render inline approval cards with diff preview; Approve applies `Op` transactions via the core queue (single undo step), audited in agent activity; Reject resolves the tool as denied.
- Timeout, panel close, thread switch or quit resolve pending approvals as denied (fail closed).

## Notes
Implements BIT-SP-0011.R2. Pando refs: `internal/agui/frontend_tool.go`, `hitl.go:40`.
