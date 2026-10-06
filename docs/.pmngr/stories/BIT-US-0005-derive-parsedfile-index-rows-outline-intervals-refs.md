---
id: BIT-US-0005
type: story
title: "Derive ParsedFile index rows: outline intervals, refs, properties and tasks"
status: done
priority: critical
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, bitacora-index]
estimate: 8
created: 2026-10-06T14:25:23Z
updated: 2026-10-06T17:32:45Z
closed: 2026-10-06T17:32:45Z
---

## Description
As a developer, I want a pure `parse(path, bytes, config) -> ParsedFile` function that turns the lossless outline from `bitacora-markdown`/`bitacora-core` into index-ready rows, so that the writer only has to persist data whose semantics already match Logseq.

It computes pre-order `ord`/`subtree_end`/`depth`/`sibling_idx`/`parent_ord`, page refs with `kind`, block refs, EAV properties, task columns (marker, priority, scheduled/deadline), `content`, `title`, `search_text` and `content_hash`.

## Acceptance Criteria
- `ParsedFile` matches the struct in [[sqlite-index-schema]] §4.3 and the function has no DB or I/O dependency.
- Refs follow Logseq: links, tags, property names/values, marker, priority, namespace parents, embeds; block refs from `((uuid))`.
- Property values typed with Logseq rules (refs set / int / bool / string, comma-separated keys from config).
- Golden tests over `fixtures/graphs/**` compare refs and properties against expected JSON.

## Notes
Implements: BIT-SP-0003.R4, BIT-SP-0003.R8, BIT-SP-0003.R10, BIT-SP-0003.R13. See [[03-parsing-indexing-search]] §3–§5, [[02-markdown-block-syntax]], [[sqlite-index-schema]] §4.3. ADR-003.
