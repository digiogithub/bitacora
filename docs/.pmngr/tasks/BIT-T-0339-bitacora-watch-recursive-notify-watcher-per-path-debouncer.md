---
id: BIT-T-0339
type: task
title: "bitacora-watch: recursive notify watcher, per-path debouncer and ignore rules"
status: backlog
priority: critical
parent: BIT-US-0067
milestone: BIT-M-0003
author: mcp
labels: [bitacora-watch, io]
estimate: 3
created: 2026-10-06T14:34:13Z
updated: 2026-10-06T14:34:13Z
---

## Description
`crates/bitacora-watch/src/lib.rs`: `GraphWatcher::start(root, ignore: IgnoreRules, sink: impl Fn(FileEvent))` using `notify` (recommended watcher) + `notify-debouncer-full` with 100 ms per-path window; after debounce read the file and `blake3`-hash it → `FileEvent { rel_path, kind: Upserted|Removed|Renamed{from}, hash, bytes }`. `IgnoreRules` shared with the indexer: dot-paths, `.git`, `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `logseq/graphs-txid.edn`, `logseq/pages-metadata.edn`, `node_modules`, `.DS_Store`, `*.bitacora-tmp`, config `:hidden` prefixes; only `.md`/`.org`/`config.edn` events are emitted. On watcher error (e.g. inotify limit) emit `WatchError` and start a 5 s mtime rescan fallback.

## Acceptance Criteria
- Integration tests in temp dirs: create/modify/delete/rename, ignored paths, burst coalescing (3 writes in 50 ms → 1 event).
- No dependency on `bitacora-app`; only `bitacora-core` types.

## Notes
Story BIT-US-0067. Implements BIT-SP-0005.R11. [[01-file-graph-layout]] §1.1, [[crate-stack]].
