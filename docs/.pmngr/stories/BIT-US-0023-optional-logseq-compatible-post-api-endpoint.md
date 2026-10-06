---
id: BIT-US-0023
type: story
title: Optional Logseq-compatible POST /api endpoint
status: backlog
priority: low
parent: BIT-EP-0010
milestone: BIT-M-0003
author: mcp
labels: [mcp, api, compat]
estimate: 3
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T14:27:12Z
---

## Description
As a power user with scripts targeting Logseq's HTTP API, I want an optional `POST /api` accepting `{method, args}`, so that my existing integrations work against Bitacora.

## Acceptance Criteria
- Disabled by default (`404`); when enabled, same bearer/Origin/Host guards and scopes as `/mcp`.
- `logseq.Editor.getPage|getBlock|getPageBlocksTree|insertBlock|appendBlockInPage|prependBlockInPage|updateBlock|moveBlock|removeBlock|createPage|renamePage|deletePage|getPageLinkedReferences|upsertBlockProperty|removeBlockProperty`, `logseq.DB.q`, `logseq.search` map onto the MCP handlers with camelCase PageEntity/BlockEntity-like JSON.
- UI-only, `Git.*`, `App.relaunch/quit`, plugin and `datascriptQuery` methods → `{"error":"method not supported"}`.
- Calls are audited like MCP calls.

## Notes
Implements: BIT-SP-0007.R17. See [[05-git-and-apis]] §2.2, [[mcp-server]] §2.
