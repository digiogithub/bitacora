---
id: BIT-US-0104
type: story
title: Block and page embeds rendered and editable in place
status: done
priority: medium
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [ui, editor, bitacora-app]
estimate: 5
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T22:20:30Z
closed: 2026-10-06T22:20:30Z
---

## Description
As a writer, I want `{{embed ((uuid))}}` and `{{embed [[page]]}}` to show the embedded outline and let me edit it in place, so that I can reuse content without copying it.

## Acceptance Criteria
- Embeds render a nested outline (block subtree or page) with a visual frame and source link.
- Editing inside an embed goes through the core command queue and updates the source file only.
- Cycle guard (an embed of an ancestor shows "circular embed") and depth limit (default 5).
- Embeds refresh on index events of the source file.

## Notes
[[block-editor]] §8 and §9 Later list; [[gpui-and-gpui-kit]] §2.3 (Block refs and embeds). ADR-002.
