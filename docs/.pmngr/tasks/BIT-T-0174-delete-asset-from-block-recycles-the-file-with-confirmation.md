---
id: BIT-T-0174
type: task
title: Delete asset from block recycles the file (with confirmation)
status: backlog
parent: BIT-US-0089
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, delete, assets]
estimate: 2
created: 2026-10-06T14:30:53Z
updated: 2026-10-06T14:30:53Z
---

## Description
When the user removes an asset link from a block through the "delete asset" action (`editor.cljs:1495-1512`), resolve the local asset path (`../assets/x.png` → `assets/x.png`), and if no other block references it (index lookup via core trait), emit `RecycleFile`. UI (`crates/bitacora-app/src/editor/asset_menu.rs`) asks for confirmation. Plain text deletion of a link never removes files (no GC).

## Acceptance Criteria
- Removing `![x.png](../assets/x.png)` via the action recycles `assets/x.png` to `logseq/.recycle/assets_x.png`.
- If another block references the asset, file is kept and the user is told.
- Deleting text manually never touches assets.

## Notes
BIT-SP-0002.R14, R15. [[01-file-graph-layout]] §5.
