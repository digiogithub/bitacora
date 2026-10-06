---
id: BIT-T-0140
type: task
title: Edit buffer flush lifecycle and cross-block arrow navigation
status: done
priority: critical
parent: BIT-US-0030
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, bitacora-core, editor]
estimate: 3
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
Implement `EditSession` in `crates/bitacora-app/src/editor/session.rs`: 500 ms idle timer → commit `EditText` (or `SetText` for large diffs) via the command queue; immediate commit on blur, `Esc`, block navigation, window deactivate and before any command/undo (`Graph::commit` flush hook). No-op when unchanged; skipped while IME composing. Navigation: Up on first visual row / Down on last row → previous/next visible block keeping x; Left@0 / Right@end → neighbour end/start; skip children of collapsed blocks.

## Acceptance Criteria
- `#[gpui::test]` with fake timers: commit after 500 ms; commit on Esc; no op when unchanged.
- Navigation tests over `- a\n  collapsed:: true\n\t- a1\n- b`: Down from `a` goes to `b`.
- Commit happens before `Tab` is applied (single history order: text then indent).

## Notes
Story BIT-US-0030. Implements BIT-SP-0004.R1, BIT-SP-0004.R3. Logseq refs `editor.cljs:1852-1868`, `:2558-2653`.
