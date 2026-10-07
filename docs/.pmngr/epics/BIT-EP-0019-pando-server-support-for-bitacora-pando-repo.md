---
id: BIT-EP-0019
type: epic
title: Pando server support for Bitacora (Pando repo)
status: cancelled
priority: high
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, pando-repo]
created: 2026-10-07T09:10:26Z
updated: 2026-10-07T09:53:36Z
closed: 2026-10-07T09:53:36Z
---

## Description
Close the gaps in Pando's server API found during v2 analysis so Bitacora can index block-level documents and run agents against its MCP server: list documents by prefix with content hash, content-hash skip on upsert, batch upsert/delete, delete by prefix, index-only documents (no mirror file), prefix-scoped tokens, confirm AG-UI agents use external MCP servers (`streamable-http` + headers) and `Tools` globs, ship Bitacora agent profiles, optional per-request MCP config, and a versioned REST contract.

## Acceptance Criteria
- Each endpoint change has Go tests and docs in Pando (`docs/agui.md`, API docs).
- `pando-rs` exposes the new endpoints.

## Notes
- Plan [[bitacora-v2-plan]]. Work in `/www/MCP/Pando/pando` (`internal/api/routes.go:155-169`, `handlers_remembrances_kb.go`, `internal/agui/`).
- Bitacora must keep working (with client-side workarounds) against a Pando without these changes until the minimum version is raised.
