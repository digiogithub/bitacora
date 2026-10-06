---
id: BIT-US-0067
type: story
title: Graph file watcher with echo suppression
status: backlog
priority: critical
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [io, watch, bitacora-watch, bitacora-core]
estimate: 5
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T14:29:01Z
---

## Description
As a user editing the same graph in Logseq or a text editor, I want Bitacora to notice external file changes within a fraction of a second, but never react to its own writes, so that views stay current without reparse loops.

## Acceptance Criteria
- `bitacora-watch`: recursive `notify` watcher + per-path 100 ms debouncer; emits `FileEvent { path, kind, hash }` after reading and hashing.
- Ignore rules: dot-paths, `.git`, `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `graphs-txid.edn`, `pages-metadata.edn`, `node_modules`, `.DS_Store`, `*.bitacora-tmp`, config `:hidden`.
- Echo filter `(path, hash)` with 5 s TTL populated by the writer; events matching it or the current `DiskSnapshot` hash are dropped.
- Watcher errors (inotify limit) produce a notice and fall back to periodic mtime rescan.
- Integration test: 100 consecutive edits trigger 0 reparses; an external write is picked up within 300 ms.

## Notes
Implements: BIT-SP-0005.R11, BIT-SP-0005.R12.
See [[block-editor]] §5.2 step 6, §6.1; [[01-file-graph-layout]] §1.1 and requirement 23; [[crate-stack]] (notify). ADR-011.
