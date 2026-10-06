---
id: BIT-T-0202
type: task
title: Implement create_page, append_block, prepend_block and insert_block
status: done
priority: high
parent: BIT-US-0020
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, tools, write]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/src/tools/write_blocks.rs`: `create_page {name, properties?, blocks?, journal?, if_exists=error|return}` → `Op::CreatePage` + `InsertBlocks`; `append_block`/`prepend_block {page|"today", content, properties?, children?}` (today resolved via config; creates the journal file on first write); `insert_block {target_uuid, content|blocks[], position=after|before|first_child|last_child, properties?}` → `Op::InsertBlocks`. Target block gets persistent `id::` lazily (core `EnsureBlockId` op in the same transaction). Return new uuids, versions and page etag.

## Acceptance Criteria
- Tool tests on a temp copy of a fixture graph verifying file bytes (indent style preserved, untouched blocks byte-identical).

## Notes
Story BIT-US-0020. Implements BIT-SP-0007.R12. Mirrors `Editor.createPage`, `appendBlockInPage`, `prependBlockInPage`, `insertBlock`, `insertBatchBlock`. ADR-006.
