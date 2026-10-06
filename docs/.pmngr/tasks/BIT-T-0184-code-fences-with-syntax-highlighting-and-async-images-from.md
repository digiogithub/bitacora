---
id: BIT-T-0184
type: task
title: Code fences with syntax highlighting and async images from assets
status: done
priority: medium
parent: BIT-US-0074
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, rendering]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:44:23Z
started: 2026-10-06T18:28:35Z
closed: 2026-10-06T18:44:23Z
---

## Description
`crates/bitacora-app/src/render/media.rs`: code fences rendered read-only with highlighting (GPUI Kit `Editor` in read-only mode, or `syntect` runs if Editor instances are too heavy per block — measure with 200 code blocks); images `![alt](../assets/x.png)` resolved relative to the graph root, loaded with GPUI `img()` asynchronously, max width = content width, broken image placeholder; remote URLs loaded only if setting `render.remote_images` (default on). Block-level Markdown (lists, quotes, tables) via `pulldown-cmark` events to GPUI elements.

## Acceptance Criteria
- Fixture page with 200 code blocks scrolls without frame drops > 32 ms (measured).
- Missing asset shows placeholder, no panic.

## Notes
[[gpui-and-gpui-kit]] §2.2 (Editor, TextView). ADR-003.
