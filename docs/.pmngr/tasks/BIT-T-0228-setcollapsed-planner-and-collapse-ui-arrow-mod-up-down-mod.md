---
id: BIT-T-0228
type: task
title: SetCollapsed planner and collapse UI (arrow, Mod+Up/Down, Mod+;, t o)
status: backlog
priority: high
parent: BIT-US-0035
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, outliner]
estimate: 3
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T14:31:33Z
---

## Description
`crates/bitacora-core/src/commands/collapse.rs`: `plan_set_collapsed(ids, bool)` emits `EditText` inserting `collapsed:: true` after the title line (or after the last existing property line, matching Logseq order) / removing it; skip leaf blocks. Page-level without target: collapse/expand one level. `t o` toggles all. In `PageView` the row list skips children of collapsed blocks and the arrow toggles.

## Acceptance Criteria
- Collapse then expand returns the exact original bytes.
- Golden case: `- a\n\t- a1` → `- a\n  collapsed:: true\n\t- a1`.
- Row list updated via `ListState::splice` only for affected range.

## Notes
Story BIT-US-0035. Implements BIT-SP-0004.R11. Logseq `editor.cljs:3482-3586`, `file/core.cljs:20-32`.
