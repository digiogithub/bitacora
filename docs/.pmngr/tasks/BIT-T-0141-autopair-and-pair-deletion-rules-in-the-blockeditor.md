---
id: BIT-T-0141
type: task
title: Autopair and pair deletion rules in the BlockEditor
status: backlog
priority: medium
parent: BIT-US-0030
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor]
estimate: 2
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T14:30:00Z
---

## Description
Implement `crates/bitacora-app/src/editor/autopair.rs` as a pure function over `(buffer, selection, typed_char) -> Edit`: pairs `[] {} () `` ~~ ** __ ^^ == ++`; typing the closing char skips over it; Backspace on an opening char with its pair immediately after deletes both; wrapping a selection with the pair. Emits the trigger used by autocomplete (`[[`).

## Acceptance Criteria
- Table-driven unit tests mirroring Logseq `editor.cljs:1565-1616`, `:2849-2948`.
- `[` → `[]`, `[[` → `[[]]` with caret in the middle; `]` before `]` skips.

## Notes
Story BIT-US-0030. Implements BIT-SP-0004.R15 (autopair part).
