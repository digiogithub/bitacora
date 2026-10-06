---
id: BIT-T-0172
type: task
title: Recycle primitive in the core writer
status: backlog
parent: BIT-US-0089
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, delete]
estimate: 2
created: 2026-10-06T14:30:53Z
updated: 2026-10-06T14:30:53Z
---

## Description
`crates/bitacora-core/src/writer/recycle.rs`: `fn recycle_path(rel: &RelPath) -> RelPath` = `logseq/.recycle/` + rel with `/` and `\` replaced by `_` (`electron/handler.cljs:51-66`). `Op::RecycleFile { path }` moves the file (create `logseq/.recycle/` if missing, overwrite existing target like `renameSync`), records the previous recycle content (if overwritten) for undo, and registers echo suppression. Pre-write hash check (ADR-011). Add a crate-level lint test that greps `crates/*/src` for `remove_file`/`remove_dir_all` outside an allowlist (temp files only).

## Acceptance Criteria
- `pages/foo.md` → `logseq/.recycle/pages_foo.md`; `pages/sub/bar.md` → `logseq/.recycle/pages_sub_bar.md`; `assets/x.png` → `logseq/.recycle/assets_x.png`.
- Overwrite case covered; undo restores both the moved file and the overwritten recycle entry.
- Lint test fails on a new `remove_file` call on graph paths.

## Notes
BIT-SP-0002.R14. [[01-file-graph-layout]] §10.3.
