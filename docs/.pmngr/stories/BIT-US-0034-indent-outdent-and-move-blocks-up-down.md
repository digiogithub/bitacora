---
id: BIT-US-0034
type: story
title: Indent, outdent and move blocks up/down
status: backlog
priority: critical
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, outliner, bitacora-core, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T14:28:07Z
---

## Description
As an outliner user, I want Tab/Shift+Tab and Alt+Shift+Up/Down to restructure my outline with Logseq semantics (direct outdenting, crossing parent boundaries), so that reorganising notes is fast and the git diff only shows moved lines.

## Acceptance Criteria
- `Indent { ids }` moves blocks as last child of the previous sibling and expands it if collapsed; no-op on first child.
- `Outdent { ids, logical }` moves after the parent; direct mode adopts following siblings as children; logical mode (setting) keeps them.
- `MoveUpDown { ids, up }` swaps with sibling and crosses parent boundaries like Logseq; no-op at page edges.
- Works on the edited block and on top-level selected blocks; non-consecutive selections refused.
- Clean moved blocks keep their original bytes except leading indentation prefixes.
- Caret/edit mode and selection follow the moved blocks.

## Notes
Implements: BIT-SP-0004.R9, BIT-SP-0004.R10.
See [[block-editor]] §3.2, [[04-editor-outliner-operations]] §3 (`indent-outdent-blocks` `core.cljs:802-854`, `move-blocks-up-down` `:760-800`). ADR-002.
