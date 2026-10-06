---
id: BIT-US-0027
type: story
title: Graph discovery, ignore rules and path normalization
status: in_progress
priority: critical
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [core, compat]
estimate: 5
created: 2026-10-06T14:28:02Z
updated: 2026-10-06T16:54:06Z
started: 2026-10-06T16:54:06Z
---

## Description
As a user opening an existing Logseq graph, I want Bitacora to find exactly the files Logseq indexes, with the same ignore rules and normalised paths, so that both apps see the same set of pages and Bitacora never touches files it does not own.

The scanner walks the graph root, skips what Logseq skips (dot-paths, symlinks, `logseq/bak`, `.recycle`, `version-files`, `node_modules`, `.DS_Store`, `:hidden` prefixes), keeps Logseq's extension whitelist, orders files like `filter-files`, normalises paths (NFC, `/` separators, graph-relative) and reads Markdown as UTF-8 with a parse-only BOM strip.

## Acceptance Criteria
- Scanning `fixtures/graphs/ignore-rules/` yields exactly the expected file list (golden test).
- `:hidden ["/archived" "test.md"]` hides `archived/**` and root `test.md` but not `pages/archived.md`.
- Stored paths are graph-relative, `/`-separated and NFC on Linux, macOS and Windows CI.
- File order matches Logseq's `filter-files` (journals reverse-sorted first, then built-ins, then the rest).
- Opening and indexing a fixture graph leaves `git status` clean (no sidecar files in the graph).
- A leading BOM is excluded from parsed text but the file bytes are never rewritten on read.

## Notes
Implements: BIT-SP-0002.R1, BIT-SP-0002.R2, BIT-SP-0002.R9, BIT-SP-0002.R16
See [[01-file-graph-layout]] §1, §1.1, §3.7, §8. ADR-005 (index outside the graph), ADR-013. [[architecture]]
