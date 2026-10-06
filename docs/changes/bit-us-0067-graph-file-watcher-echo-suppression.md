---
created_at: 2026-10-06T18:47:20.498663605Z
updated_at: 2026-10-06T18:47:20.498663605Z
tags:
    - change
    - watch
---
# BIT-US-0067 Graph file watcher with echo suppression

Part of [[bitacora-full-development-plan]]; see [[changes/bit-us-0029-0062-core-editor-model-ops-and-command-queue.md]], [[changes/bit-us-0006-0007-index-writer-and-reconcile.md]].

## What
New `crates/bitacora-watch` implementation (commit accd708): `GraphWatcher::start(root, IgnoreRules, EchoFilter, WatchConfig, sink)`, `WatchEvent {File(FileEvent{rel_path, kind Upserted|Removed|Renamed{from}, hash, bytes}), Rescan, Notice}`, `IgnoreRules` (core `scan::is_ignored_path` + node_modules at any level + `.bitacora-tmp` + `:hidden` predicate; only md/markdown/org and logseq/config.edn emitted), `EchoFilter` (path+content hash, TTL 5 s, injectable clock, not consumed on match; `record_written_file(&WrittenFile)`, `record_deleted(&GraphPath)`), polling fallback (mtime+len, 5 s) on watcher start failure or MaxFilesWatch. Events are resolved by inspecting the FS after debounce (handles delete+create saves); unchanged-hash touches are dropped; directory create/remove expands to files.

## Glue still to do (app/cli, BIT-T-0341)
Feed EchoFilter from core QueueEvent::Flushed/FilesApplied observers (register before/at write); map FileEvent to ExternalFileChanged command / index FsChange; Rescan -> FsChange::Overflow; compare against page DiskSnapshot hash and whitespace-trim no-op logic in core.

## Verification
cargo test -p bitacora-watch: 6 unit + 11 integration tests (real temp dirs, 5 repeated runs stable); clippy -D warnings, fmt, check-deps, cargo deny, typos, machete clean.
