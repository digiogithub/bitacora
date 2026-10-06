---
id: BIT-T-0329
type: task
title: Drops into right sidebar, references and embeds
status: done
priority: low
parent: BIT-US-0106
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, editor, ui]
estimate: 2
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T22:30:36Z
closed: 2026-10-06T22:30:36Z
---

## Description
Extend drop targets to blocks rendered in the right sidebar, linked-references groups and embeds (target resolves to the source page/block). Dropping onto a page title in the sidebar or All pages appends as last top-level block.

## Acceptance Criteria
- `#[gpui::test]`: drag from main view to a sidebar page moves the block to that page (both files updated, one undo).

## Notes
[[04-editor-outliner-operations]] Requirements 17.
