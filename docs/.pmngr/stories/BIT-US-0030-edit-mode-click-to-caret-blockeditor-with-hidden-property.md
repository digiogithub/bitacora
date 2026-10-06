---
id: BIT-US-0030
type: story
title: "Edit mode: click-to-caret BlockEditor with hidden-property projection and buffer flush"
status: backlog
priority: critical
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, ui, bitacora-app, bitacora-core]
estimate: 8
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T14:28:07Z
---

## Description
As a Logseq user, I want to click any rendered block and edit its raw Markdown with the caret exactly where I clicked, without seeing `id::`/`collapsed::` noise, so that editing feels like Logseq and never loses hidden metadata.

Covers the custom GPUI `BlockEditor` element (one block in edit mode, all others rendered), the `EditProjection` that hides built-in properties and the LOGBOOK drawer, the 500 ms debounced commit plus commit on blur/Esc/navigation/before commands, and arrow-key navigation across blocks.

## Acceptance Criteria
- Clicking rendered text enters edit mode with the caret at the mapped source offset (clicks on hidden markup snap to the nearest offset).
- Hidden properties and `:LOGBOOK:` are not shown; after editing the visible text they are re-inserted at their original positions (byte-exact for untouched lines).
- Buffer commits after 500 ms idle, on blur, `Esc`, block navigation, window deactivate, and before any command/undo; unchanged buffers produce no op.
- Up/Down at the first/last visual row moves to the previous/next visible block keeping x; Left at 0 / Right at end moves to neighbour end/start.
- Blocks over 10,000 chars are selected instead of edited.
- Autopair `[] {} () `` ~~ ** __ ^^ == ++` with skip-over and pair deletion.

## Notes
Implements: BIT-SP-0004.R1, BIT-SP-0004.R2, BIT-SP-0004.R3.
See [[block-editor]] §2.3, §7.1–7.2; [[04-editor-outliner-operations]] §2; [[gpui-and-gpui-kit]]. ADR-001, ADR-002.
