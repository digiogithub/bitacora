---
id: BIT-T-0309
type: task
title: Render block and page embeds with cycle guard and depth limit
status: done
priority: medium
parent: BIT-US-0104
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, ui, rendering]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T22:20:30Z
closed: 2026-10-06T22:20:30Z
---

## Description
`crates/bitacora-app/src/render/embed.rs`: `EmbedView` for `{{embed ((uuid))}}` (subtree via `IndexReader::subtree`) and `{{embed [[page]]}}` (page outline, lazy). Framed container with a source link; embed stack passed down the render tree to detect cycles (show "Circular embed") and enforce `render.embed_max_depth` (default 5). Refresh on `IndexEvent`s for the source file.

## Acceptance Criteria
- `#[gpui::test]`: page embedding itself shows the circular message; 6-level nested embeds stop at 5.

## Notes
[[block-editor]] §8; [[gpui-and-gpui-kit]] §2.3.
