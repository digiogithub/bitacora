---
id: BIT-T-0118
type: task
title: Shared tool DTOs, error codes, output rendering and pagination
status: done
priority: high
parent: BIT-US-0017
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, read]
estimate: 3
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T19:01:03Z
started: 2026-10-06T18:44:25Z
closed: 2026-10-06T19:01:03Z
---

## Description
`crates/bitacora-mcp/src/dto.rs`: `schemars` types `PageDto`, `BlockDto { uuid, content, properties, marker, version, children }`, `SearchHit`, `Cursor` (opaque base64 of index keyset), `GraphArg`. `src/errors.rs`: `ToolErrorCode` (`NOT_FOUND, CONFLICT, BLOCK_BUSY, BLOCK_IN_CONFLICT, READ_ONLY, FORBIDDEN_SCOPE, INVALID_CONTENT, INVALID_QUERY, RATE_LIMITED`) mapped to `CallToolResult` with `isError: true` and structured `{code, message, ...}`. `src/render.rs`: Markdown text rendering of trees with 20k-char cap and `truncated` flag. Graph resolution helper (`graph` arg → active graph).

## Acceptance Criteria
- Unit tests for rendering cap, cursor round-trip and error mapping.
- Output schemas published per tool (`outputSchema`).

## Notes
Story BIT-US-0017. Implements BIT-SP-0007.R16.
