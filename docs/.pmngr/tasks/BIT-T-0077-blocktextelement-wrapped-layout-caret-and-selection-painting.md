---
id: BIT-T-0077
type: task
title: "BlockTextElement: wrapped layout, caret and selection painting"
status: done
priority: critical
parent: BIT-US-0040
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:47Z
updated: 2026-10-06T17:59:48Z
started: 2026-10-06T17:09:41Z
closed: 2026-10-06T17:59:48Z
---

## Description
In `crates/bitacora-app/src/spike/block_editor/element.rs` implement a custom GPUI `Element` (`request_layout` / `prepaint` / `paint`) derived from `examples/input.rs`:
- State entity `SpikeBlockEditor { text: String, selection: Range<usize> /* UTF-8 */, reversed: bool, marked_range: Option<Range<usize>>, goal_x: Option<Pixels>, focus_handle }`.
- Layout via `window.text_system().shape_text(...)` -> `WrappedLine`s with soft wrap at the available width; `TextRun`s for syntax colouring (dim `[[`/`]]`, bold `TODO`, underline for marked text).
- Paint selection quads across wrapped rows, caret (blink with a timer), and hit testing with `index_for_position`/`position_for_index` for mouse down/drag/shift-click.
- Cache shaped lines per (text hash, width) to avoid re-shaping on caret moves.

## Acceptance Criteria
- Mouse click/drag selects the right ranges in multi-line wrapped text including CJK and emoji (grapheme-safe caret movement via `unicode-segmentation`).
- Resizing the window re-wraps without losing the selection.
- `#[gpui::test]` covering position <-> index round trips on wrapped lines.

## Notes
- [[gpui-and-gpui-kit]] §3.2 items 2–3, §3.3; [[block-editor]] §7.2. Apache-2.0 sources only (ADR-014).
