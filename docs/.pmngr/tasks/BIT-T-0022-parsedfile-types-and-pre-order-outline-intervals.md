---
id: BIT-T-0022
type: task
title: ParsedFile types and pre-order outline intervals
status: backlog
priority: critical
parent: BIT-US-0005
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T14:26:33Z
---

## Description
`crates/bitacora-index/src/parsed.rs`: define `ParsedFile`, `PageDef`, `ParsedBlock`, `PageRefName`, `Diagnostic` as in [[sqlite-index-schema]] §4.3. `src/derive/mod.rs`: `pub fn parse(path: &RelPath, bytes: &[u8], cfg: &GraphConfig) -> ParsedFile` built on the `bitacora-markdown` outline and `bitacora-core` page-name/journal rules (do not reimplement naming). Compute per block: `ord` (pre-block = 0), `subtree_end`, `depth` (1 = top), `sibling_idx`, `parent_ord`, `byte_start/end`, `line_start`, `content` (de-indented raw), `title` (first line without marker/priority), `collapsed`, `heading`, `content_hash = blake3(content)[..16]`, `explicit_uuid` from `id::`/`custom-id` when a valid UUID.

## Acceptance Criteria
- Unit tests on irregular indentation and heading-only blocks: `subtree_end` equals the last descendant's `ord`; intervals are properly nested.
- `parse` is pure: no I/O, deterministic for the same input (proptest).

## Notes
BIT-SP-0003.R4. [[03-parsing-indexing-search]] §5, ADR-003, ADR-006.
