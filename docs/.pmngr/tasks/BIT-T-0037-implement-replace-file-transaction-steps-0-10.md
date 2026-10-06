---
id: BIT-T-0037
type: task
title: Implement replace_file transaction (steps 0-10)
status: backlog
priority: critical
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 5
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T14:27:35Z
---

## Description
`crates/bitacora-index/src/writer/replace.rs`: `fn replace_file(tx: &Transaction, pf: &ParsedFile, stat: &FileStat) -> Result<ReplaceReport>` executing [[sqlite-index-schema]] §4.4 under `BEGIN IMMEDIATE`:
0. collect GC candidates and old blocks for carry-over; 1. upsert `files` (`ON CONFLICT(path)`); 2. delete blocks/aliases/tags/diagnostics of the file; 3. demote a page the file no longer defines; 4. upsert defined page and namespace parents (`pages.uuid = UUIDv5(NS_BITACORA_PAGE, name)`, first file wins → `duplicate_page`); 5. upsert referenced pages and build name→id map; 6. insert blocks in pre-order resolving `parent_id`; 7. insert `block_page_refs`, `block_block_refs`, `block_properties`, `block_property_values`; 8. `page_aliases`/`page_tags` with `source_file_id`; 9. recompute `search_title` of the page and its alias pages; 10. GC. Use prepared cached statements.

## Acceptance Criteria
- Tests for: title change demotes old page; duplicate title diagnostic; refs/properties cascade on replace; alias page search_title updated.
- A failure mid-transaction leaves the previous state intact (inject error test).

## Notes
BIT-SP-0003.R5, BIT-SP-0003.R11.
