---
id: BIT-T-0205
type: task
title: Scope enforcement, read-only default and tool catalogue filtering
status: backlog
priority: high
parent: BIT-US-0021
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, security, write]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T14:31:00Z
---

## Description
`crates/bitacora-mcp/src/policy.rs`: `Policy::check(ctx: &AuthContext, tool: ToolClass{Read|Write|Delete}, settings: &McpSettings) -> Result<(), ToolError>` → `READ_ONLY` when `allow_writes`/`allow_deletes` is false, `FORBIDDEN_SCOPE` when token lacks scope. Call it at the start of every write/delete handler (macro or wrapper). `tools/list` filtered per token scope (write/delete tools omitted for read-only tokens). Optional `confirm_writes` stub: returns `READ_ONLY` with message "awaiting user confirmation not yet supported" until UX is designed.

## Acceptance Criteria
- Tests: fresh install append → `READ_ONLY` and file unchanged; token [read, write] calling `remove_block` → `FORBIDDEN_SCOPE`; read token's tools/list contains only read tools.

## Notes
Story BIT-US-0021. Implements BIT-SP-0007.R6.
