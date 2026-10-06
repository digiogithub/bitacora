---
id: BIT-T-0080
type: task
title: Unfocused block rendering with click-to-caret offset mapping
status: done
priority: high
parent: BIT-US-0040
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, bitacora-app]
estimate: 3
created: 2026-10-06T14:28:47Z
updated: 2026-10-06T17:59:48Z
started: 2026-10-06T17:09:41Z
closed: 2026-10-06T17:59:48Z
---

## Description
For non-editing blocks build a display string plus `TextRun`s with a tiny regex-based inline scanner (spike-only; the real tokenizer is BIT-EP-0003): `[[page]]` shown as a link-styled "page" (brackets hidden), `#tag` styled, `TODO`/`DONE` markers styled, `**bold**` with markers hidden. Record a `Vec<(display_range, source_range)>` map. Render with `InteractiveText::new(id, StyledText::new(display).with_runs(runs))`; `on_click` on a ref range logs navigation; a click elsewhere enters edit mode with the caret at the mapped source offset (clicks on hidden markup snap to the nearest offset).

## Acceptance Criteria
- Unit tests for display->source mapping across hidden markup and multi-byte characters.
- Clicking the middle of a rendered `[[Some page]]` link triggers navigation; clicking plain text after it opens the editor with the caret at the right source byte (`#[gpui::test]` with simulated click).

## Notes
- [[block-editor]] §7.1–7.2 ("Click to caret"), [[gpui-and-gpui-kit]] §2.3 (inline rendering of refs), §3.2 item 4, §3.3.
