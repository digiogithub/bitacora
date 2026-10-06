---
id: BIT-T-0055
type: task
title: Graph scanner with Logseq filter-files ordering and UTF-8/BOM reader
status: backlog
priority: critical
parent: BIT-US-0027
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 3
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:28:27Z
---

## Description
Add `crates/bitacora-core/src/graph/scan.rs`:
- `scan(root, cfg) -> Vec<ScannedFile>` walking recursively (e.g. `walkdir` with `follow_links(false)`), skipping symlinks and dot entries early (`common/graph.cljs:44-64`), applying `ignore.rs`.
- Order like `graph_parser.cljs:133-148` (`filter-files`): `journals/` files first in reverse-sorted order, then built-ins (`pages/contents.*`, `*.edn`, `logseq/custom.css`), then the rest sorted. This order decides duplicate-title winners.
- `read_text(path) -> SourceText { bytes, text_offset }`: read raw bytes, validate UTF-8 (report invalid files, do not abort scan), record a leading U+FEFF as `bom: true` and expose the parse slice after it. Never write anything.

## Acceptance Criteria
- Golden test over `fixtures/graphs/ignore-rules/` (see sibling fixture task) comparing ordered paths.
- Invalid UTF-8 file produces a diagnostic, scan continues.
- BOM file: parse slice starts after `EF BB BF`; raw bytes unchanged.

## Notes
Refs BIT-SP-0002.R2, BIT-SP-0002.R16. [[01-file-graph-layout]] §1.1, §8.
