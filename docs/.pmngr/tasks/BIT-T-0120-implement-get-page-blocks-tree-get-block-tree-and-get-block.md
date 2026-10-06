---
id: BIT-T-0120
type: task
title: Implement get_page_blocks_tree / get_block_tree and get_block tools
status: in_progress
priority: high
parent: BIT-US-0017
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, read]
estimate: 3
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
`crates/bitacora-mcp/src/tools/read_blocks.rs`: `get_page_blocks_tree {name|uuid, max_depth?, include_properties=true, collapsed_children=true}` (registered also as `get_block_tree` when called with uuid) returning nested `BlockDto` + `markdown`; `get_block {uuid, include_children=false, include_parents=false}` returning block + breadcrumb. Reads from the index snapshot (WAL) and never block on writes. Blocks without persistent `id::` get a session handle uuid (documented as not stable across restarts until written).

## Acceptance Criteria
- Tests: depth limit, collapsed children, breadcrumb order, versions present, truncation flag on a large page fixture.

## Notes
Story BIT-US-0017. Implements BIT-SP-0007.R11, BIT-SP-0007.R16. Mirrors `Editor.getPageBlocksTree`, `Editor.getBlock`. ADR-006.
