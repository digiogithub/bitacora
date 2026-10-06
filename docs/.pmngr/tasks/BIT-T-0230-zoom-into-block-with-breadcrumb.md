---
id: BIT-T-0230
type: task
title: Zoom into block with breadcrumb
status: backlog
priority: medium
parent: BIT-US-0035
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, ui]
estimate: 2
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T14:31:33Z
---

## Description
In `crates/bitacora-app/src/views/page_view.rs` add a `zoom_root: Option<BlockId>` view state; `Mod+.`/`Alt+Right`/bullet click sets it to the current block, `Mod+,`/`Alt+Left` moves to the parent (None at top). Breadcrumb component shows page title › ancestors, each clickable. Outdent of zoom-root children beyond the root is refused. Zoom is pushed onto navigation history (`Mod+[`/`Mod+]`).

## Acceptance Criteria
- `#[gpui::test]`: zoom in/out updates visible rows and breadcrumb; no file write happens.
- Zoom root deleted externally → zoom resets to page.

## Notes
Story BIT-US-0035. Implements BIT-SP-0004.R19. Logseq `editor.cljs:1188-1200`.
