---
id: BIT-T-0222
type: task
title: ImportAsset op and paste/drop handling in the block editor
status: backlog
parent: BIT-US-0096
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, assets]
estimate: 3
created: 2026-10-06T14:31:16Z
updated: 2026-10-06T14:31:16Z
---

## Description
Core: `Op::ImportAsset { bytes|source_path, original_name, epoch_ms, index }` writes `assets/<name>` atomically (temp + fsync + rename, ADR-011; create `assets/` on first use) and returns the link text; combined with `Op::EditBlock` inserting the link at the cursor in one transaction (undo removes the file to recycle and the text). App: `crates/bitacora-app/src/editor/paste.rs` handles clipboard images (`image.png`) and file drops (multiple files → indices 0..n with the same ms), and renders local images via `resolve_local_asset`.

## Acceptance Criteria
- Pasting a clipboard PNG inserts `![image.png](../assets/image_<ms>_0.png)` and writes the file.
- Dropping 2 files yields indices `_0`, `_1`.
- Undo removes the link and recycles the imported file.
- `#[gpui::test]` for drop handling with a fake clock.

## Notes
BIT-SP-0002.R15. [[block-editor]].
