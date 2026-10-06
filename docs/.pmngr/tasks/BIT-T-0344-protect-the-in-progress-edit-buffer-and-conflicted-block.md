---
id: BIT-T-0344
type: task
title: Protect the in-progress edit buffer and conflicted-block choice UI
status: backlog
priority: high
parent: BIT-US-0068
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, bitacora-core, editor]
estimate: 3
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T14:34:14Z
---

## Description
When `Reloaded`/merge touches the block in edit mode: `EditSession` keeps its buffer and caret, records `conflicted_disk_text`, and shows a conflict indicator on the block. On commit (Esc/blur/debounce suppressed while conflicted) show an inline chooser: Keep mine (SetText ours), Take disk (discard buffer), Keep both (ours + disk text inserted as next sibling). Changes to other blocks apply without touching the buffer or caret.

## Acceptance Criteria
- `#[gpui::test]`: other-block change keeps caret offset; same-block change shows chooser; "Keep both" yields `- a-mine\n- a-theirs`.
- Debounced auto-commit does not silently overwrite while conflicted.

## Notes
Story BIT-US-0068. Implements BIT-SP-0005.R14. [[block-editor]] §6.3; Logseq has no protection (`watcher_handler.cljs:44-56`).
