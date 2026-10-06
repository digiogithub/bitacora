---
id: BIT-US-0021
type: story
title: Write/delete scopes, rate limits, protected pages and delete tools
status: done
priority: high
parent: BIT-EP-0010
milestone: BIT-M-0003
author: mcp
labels: [mcp, security, write]
estimate: 8
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T19:40:57Z
closed: 2026-10-06T19:40:57Z
---

## Description
As a user, I want agent writes off by default and fine-grained control per token (read / write / delete), rate limits and protected pages, so that a misbehaving agent cannot mass-edit or delete my notes.

Delete tools: `remove_block`, `rename_page` (with link rewrite), `delete_page`.

## Acceptance Criteria
- `mcp.allow_writes` / `mcp.allow_deletes` default false → `READ_ONLY`; missing token scope → `FORBIDDEN_SCOPE`; nothing written.
- 60 write ops/min/token and 200 blocks/call limits → `RATE_LIMITED` / `INVALID_CONTENT`.
- Pages with `bitacora-agent-readonly:: true` or in `mcp.protected_namespaces` refuse writes.
- Delete tools carry `destructiveHint`; `rename_page` rewrites links in one undo transaction; `delete_page` follows the core delete behaviour (`logseq/.recycle`-equivalent).
- No tool executes shell/git commands or takes a raw writable file path.

## Notes
Implements: BIT-SP-0007.R6, BIT-SP-0007.R12, BIT-SP-0007.R14, BIT-SP-0007.R15. See [[mcp-server]] §3, §5.3, [[01-file-graph-layout]].
