---
id: BIT-T-0064
type: task
title: "IndexReader: page outline, subtree, ancestors and page lookups"
status: done
priority: high
parent: BIT-US-0008
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:39:44Z
closed: 2026-10-06T18:39:44Z
---

## Description
`crates/bitacora-index/src/read/outline.rs` on `IndexReader` (uses `ReadConn`): `page_by_name(name)`, `page_by_uuid`, `all_pages(filter, sort)`, `journals(before_day, limit)`, `outline(page_id, offset, limit, skip_collapsed)` (query in [[sqlite-index-schema]] §5 with the collapsed NOT EXISTS), `subtree(uuid)`, `ancestors(uuid)`, `block(uuid)`. Return domain structs `PageRow`, `BlockRow` (uuid, depth, content, title, marker, priority, collapsed, properties, byte span) — no `rusqlite` types.

## Acceptance Criteria
- Tests matching BIT-SP-0003.R4 scenarios (subtree A/B/C/D, breadcrumb, collapsed skip).
- `outline` pagination: offset 50 limit 25 returns the next visible blocks in order.
- `EXPLAIN QUERY PLAN` for outline/subtree uses `blocks_page` / `UNIQUE(file_id, ord)` indexes (asserted in test).

## Notes
BIT-SP-0003.R4.
