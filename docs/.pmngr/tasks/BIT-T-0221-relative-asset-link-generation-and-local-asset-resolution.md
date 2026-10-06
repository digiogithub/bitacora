---
id: BIT-T-0221
type: task
title: Relative asset link generation and local-asset resolution
status: backlog
parent: BIT-US-0096
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, assets]
estimate: 2
created: 2026-10-06T14:31:16Z
updated: 2026-10-06T14:31:16Z
---

## Description
`crates/bitacora-core/src/assets/links.rs`:
- `fn asset_link(page_path: Option<&RelPath>, asset: &RelPath, original_name: &str, kind) -> String`: relative path from the page file's directory (`../assets/…` for `pages/x.md`/`journals/x.md`, `../../assets/…` for `pages/sub/x.md`; base `pages/_.md` when the page has no file, `editor.cljs:1515-1528`); `![name](…)` for image/audio/video/PDF, `[name](…)` otherwise (`handler/assets.cljs:117-128`).
- `fn resolve_local_asset(page_path, link) -> Option<PathBuf>`: local if matches `^[./]*assets` (`graph_parser/config.cljs:18-21`); resolve relative to the file, fallback to `<root>/assets/…`. `@alias/…` returns `None` (preserved verbatim).

## Acceptance Criteria
- Unit tests for each link form and depth; `../assets/a.png` from `pages/sub/x.md` resolves via fallback to `<root>/assets/a.png`; `@alias/x.pdf` unresolved and unchanged.

## Notes
BIT-SP-0002.R15.
