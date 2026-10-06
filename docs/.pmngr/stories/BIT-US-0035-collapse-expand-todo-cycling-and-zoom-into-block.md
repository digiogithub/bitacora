---
id: BIT-US-0035
type: story
title: Collapse/expand, TODO cycling and zoom into block
status: done
priority: high
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, outliner, bitacora-core, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T21:12:42Z
started: 2026-10-06T20:22:06Z
closed: 2026-10-06T21:12:42Z
---

## Description
As a Logseq user, I want to collapse subtrees (persisted as `collapsed:: true`), cycle task markers with Mod+Enter or the checkbox, and zoom into a block with a breadcrumb, so that I can focus on part of a page and manage tasks as before.

## Acceptance Criteria
- `SetCollapsed { ids, bool }` adds/removes the `collapsed:: true` line via `EditText`; leaf blocks are ignored; page-level one-level collapse/expand without target; `t o` toggles all.
- Collapse arrow click, `Mod+Up`, `Mod+Down`, `Mod+;` wired in editor and selection contexts.
- `CycleMarker { ids }` follows `:preferred-workflow` (`TODO→DOING→DONE→none` or `LATER→NOW→DONE→none`); checkbox click toggles DONE; priority and properties untouched.
- Zoom in (`Mod+.`, `Alt+Right`, bullet click) and out (`Mod+,`, `Alt+Left`) re-root the view with a breadcrumb; no file change.
- Expand after collapse restores the original bytes.

## Notes
Implements: BIT-SP-0004.R11, BIT-SP-0004.R14, BIT-SP-0004.R19.
See [[block-editor]] §3.2, §9 items 7, 10, 15; [[04-editor-outliner-operations]] §3 (`set-blocks-collapsed!`, `cycle-todo!`, `zoom-in!`); [[02-markdown-block-syntax]] for markers.
