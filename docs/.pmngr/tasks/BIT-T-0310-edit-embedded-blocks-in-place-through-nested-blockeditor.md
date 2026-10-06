---
id: BIT-T-0310
type: task
title: Edit embedded blocks in place through nested BlockEditor
status: backlog
priority: medium
parent: BIT-US-0104
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, editor, bitacora-core]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
Allow click-to-edit inside `EmbedView` by hosting nested `BlockEditor` instances bound to the source page's document (`bitacora-core`), so ops target the source file; keyboard navigation up/down crosses embed boundaries like Logseq; undo groups per source transaction.

## Acceptance Criteria
- `#[gpui::test]`: editing an embedded block updates only the source file; undo restores exact bytes.
- The host page file is never written by an embed edit.

## Notes
ADR-002. [[block-editor]] §9 Later ("Embeds edited in place").
