---
id: BIT-T-0212
type: task
title: POST /api method dispatcher mapping Logseq methods to tool handlers
status: done
priority: low
parent: BIT-US-0023
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, api, compat]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/src/compat_api.rs`: axum route `POST /api` mounted only when `mcp.compat_api = true`, behind the same guard. Parse `{method, args}`; resolve `logseq.<NS>.<camelName>` like Logseq (`server.cljs:58-72`); whitelist table mapping to internal handlers with positional-arg adapters (e.g. `insertBlock(target, content, {before, sibling, properties})` → `insert_block` position). Responses camelCase PageEntity/BlockEntity-like JSON; errors `{"error": msg}` with HTTP 200 (Logseq-compatible). Unsupported/forbidden methods → `{"error":"method not supported"}`. Audit every call.

## Acceptance Criteria
- HTTP tests: disabled → 404; `getPage` works; `logseq.Git.execCommand` refused; scopes enforced for write methods.

## Notes
Story BIT-US-0023. Implements BIT-SP-0007.R17. See [[05-git-and-apis]] §2.2.
