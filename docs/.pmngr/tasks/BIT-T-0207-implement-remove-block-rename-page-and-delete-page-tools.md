---
id: BIT-T-0207
type: task
title: Implement remove_block, rename_page and delete_page tools
status: done
priority: high
parent: BIT-US-0021
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, tools, delete]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/src/tools/delete.rs` (scope `delete`, `destructiveHint: true`): `remove_block {uuid, expected_version?}` (subtree) → `Op::DeleteBlocks`; `rename_page {name, new_name, update_links=true}` → core rename cascade (file rename per `:file/name-format`, `title::`, link rewrites) in one transaction; `delete_page {name}` → core delete (Logseq `.recycle`-equivalent). Page names only map to files via core naming service; no raw paths.

## Acceptance Criteria
- Tests on temp graph: rename rewrites `[[Old]]` in other pages and is undone by a single undo; delete moves file per core behaviour; traversal-like names stay inside `pages/`.

## Notes
Story BIT-US-0021. Implements BIT-SP-0007.R12, BIT-SP-0007.R15. Mirrors `Editor.removeBlock`, `renamePage`, `deletePage`. See [[01-file-graph-layout]].
