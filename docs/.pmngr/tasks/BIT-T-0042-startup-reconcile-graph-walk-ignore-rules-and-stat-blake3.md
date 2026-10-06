---
id: BIT-T-0042
type: task
title: "Startup reconcile: graph walk, ignore rules and stat/blake3 change filter"
status: done
priority: critical
parent: BIT-US-0007
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
`crates/bitacora-index/src/pipeline/reconcile.rs`: walk the graph with `ignore`/`walkdir` applying Logseq ignore rules (dot-paths, `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `node_modules`, `graphs-txid.edn`, `pages-metadata.edn`) and `:hidden` (reuse the predicate from `bitacora-core` if present). Collect `{path (NFC, '/'), size, mtime_ns}`; compute `db − disk` → `Delete` jobs; for each disk file: equal `(size, mtime_ns)` → skip (unless `paranoid_scan`), else read + blake3; equal hash → `touch` metadata only; else enqueue parse.

## Acceptance Criteria
- Test: unchanged graph → zero reads (count via an injected FS trait).
- Test: `git checkout`-style mtime change with same bytes → metadata update only, block rowids unchanged.
- Test: files under `logseq/bak/` and `.git/` never appear in `files`.

## Notes
BIT-SP-0003.R6. [[sqlite-index-schema]] §4.1 steps 2-4; [[03-parsing-indexing-search]] §1.1.
