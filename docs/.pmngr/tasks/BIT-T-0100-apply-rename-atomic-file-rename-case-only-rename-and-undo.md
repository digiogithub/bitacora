---
id: BIT-T-0100
type: task
title: "Apply rename: atomic file rename, case-only rename and undo"
status: backlog
parent: BIT-US-0061
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, rename]
estimate: 3
created: 2026-10-06T14:29:22Z
updated: 2026-10-06T14:29:22Z
---

## Description
Implement `Op::RenameFile { from, to }` in the core writer (`crates/bitacora-core/src/writer/rename.rs`): pre-check that `from` hash matches last-known (ADR-011), `to` does not exist, then `std::fs::rename`. Case-only on case-insensitive FS (macOS/Windows): rename via temp name `<stem>.bitacora-tmp-<rand>` then final. Register paths with watcher echo suppression. Inverse op for undo. The plan from the planner is applied as one transaction: renames, `title::` rewrite, config updates (other task) and ref rewrites (other story).

## Acceptance Criteria
- Integration test on tempdir: rename `pages/Old.md` → `pages/New Idea.md` preserves bytes; undo restores.
- Case-only `pages/foo.md` → `pages/Foo.md` works on a case-insensitive FS (CI macOS/Windows) and Linux.
- Failure mid-transaction (simulated) rolls back already-renamed files.

## Notes
BIT-SP-0002.R13. ADR-011 single writer + atomic writes. [[04-editor-outliner-operations]].
