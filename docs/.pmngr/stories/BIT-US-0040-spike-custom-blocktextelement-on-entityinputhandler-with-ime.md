---
id: BIT-US-0040
type: story
title: "Spike: custom BlockTextElement on EntityInputHandler with IME"
status: done
priority: critical
parent: BIT-EP-0002
milestone: BIT-M-0001
author: mcp
labels: [ui, spike, block-editor, bitacora-app]
estimate: 13
created: 2026-10-06T14:28:16Z
updated: 2026-10-06T17:59:48Z
started: 2026-10-06T17:06:16Z
closed: 2026-10-06T17:59:48Z
---

## Description
As a developer deciding ADR-002, I want a throwaway but realistic prototype of the custom block text element — multi-line soft wrap, caret, selection, IME composition and cross-block navigation — so that we know whether Option C (custom element on `EntityInputHandler`) is viable before committing the editor epic to it.

Code lives in `crates/bitacora-app/src/spike/block_editor/` behind a `--spike-editor` flag (or a `spike` cargo feature) and uses an in-memory `Vec<SpikeBlock { text: String, depth: u8 }>` instead of `bitacora-core`.

## Acceptance Criteria
- Focused block: typing, soft wrap at container width, caret painting and blinking, mouse selection and shift+arrow selection, clipboard copy/paste of plain text.
- `EntityInputHandler` fully implemented (UTF-8 <-> UTF-16 conversion, marked text, `bounds_for_range` correct so IME candidate windows appear at the caret).
- Up on first visual row / Down on last visual row moves to the adjacent block keeping `goal_x`; Enter splits; Backspace at offset 0 merges with the previous block; Tab/Shift-Tab change depth.
- Unfocused blocks render as `StyledText` with dimmed `[[`/`]]` and a styled `TODO`; clicking puts the caret at the matching source offset.
- Keystroke-level `#[gpui::test]` suite covers the behaviours above.

## Notes
- [[gpui-and-gpui-kit]] §1.2, §3.1 Option C, §3.2, §3.3; [[block-editor]] §7.1–7.3.
- ADR-002 (decision to validate). Start from GPUI's `examples/input.rs` (Apache-2.0); borrow ideas from GPUI Kit's Input (Apache-2.0, attribution); never from Zed's GPL `editor` crate (ADR-014).
- Spike code is not production; BIT-EP-0007 re-implements on top of `bitacora-core`.
