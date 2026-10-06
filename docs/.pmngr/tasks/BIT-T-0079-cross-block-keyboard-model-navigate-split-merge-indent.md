---
id: BIT-T-0079
type: task
title: "Cross-block keyboard model: navigate, split, merge, indent"
status: in_progress
priority: high
parent: BIT-US-0040
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, bitacora-app]
estimate: 3
created: 2026-10-06T14:28:47Z
updated: 2026-10-06T17:09:41Z
started: 2026-10-06T17:09:41Z
---

## Description
In `spike/block_editor/page.rs` add a `SpikePage` entity holding `Vec<SpikeBlock>` and the index of the editing block; only one live `SpikeBlockEditor` exists at a time (Option C). Actions in key context `"BlockEditor"`: `MoveUp`/`MoveDown` (at first/last visual row jump to the neighbour, preserving `goal_x`), `MoveLeft` at 0 / `MoveRight` at end (to neighbour end/start), `Newline` (split at caret), `DeleteBackward` at 0 (merge into previous, caret at join point), `Indent`/`Outdent` (depth +-1 with Logseq's constraints: cannot indent the first sibling), `ShiftEnter` (soft line break), `Escape` (leave edit mode, select block). Flush buffer into the block vector on blur/navigation (mirrors the 500 ms/blur flush rule).

## Acceptance Criteria
- `#[gpui::test]` keystroke sequences: split then merge restores the original text; up/down across 3 blocks keeps the x column within one glyph; Tab on the first block is a no-op.
- Behaviour matches the cited Logseq rules for each action (references in test names/comments).

## Notes
- [[block-editor]] §7.2 (vertical navigation, buffer flush), §7.3 (key contexts); [[gpui-and-gpui-kit]] §3.2 item 5; Logseq `editor.cljs:2558-2653` as cited in [[block-editor]].
