---
id: BIT-T-0074
type: task
title: Diagnostics read API and DiagnosticKind enum
status: done
priority: medium
parent: BIT-US-0010
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 1
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:39:52Z
closed: 2026-10-06T18:39:52Z
---

## Description
`crates/bitacora-index/src/read/diagnostics.rs`: `enum DiagnosticKind { ParseError, DuplicatePage, DuplicateBlockId, InvalidProperty, CaseConflict, TooLarge }` with `as_str` matching the `diagnostics.kind` column; `diagnostics(filter{file, kind, min_severity})`, `diagnostic_counts()`. Serializable with `serde` for MCP and CLI JSON output.

## Acceptance Criteria
- Tests matching BIT-SP-0003.R11 scenarios (duplicate title, duplicate block id) read back through the API.

## Notes
BIT-SP-0003.R11.
