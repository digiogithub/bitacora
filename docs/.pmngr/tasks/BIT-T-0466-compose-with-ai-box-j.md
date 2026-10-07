---
id: BIT-T-0466
type: task
title: Compose with AI box (⌘J)
status: backlog
priority: low
parent: BIT-US-0153
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, editor]
estimate: 5
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T09:19:06Z
---

## Description
Block-anchored popover with prompt, context chips, streaming amber preview from `bitacora-writer`, Accept (insert below) / Replace / Discard; keybinding ⌘J / Ctrl+J.

## Acceptance Criteria
- `#[gpui::test]`: discard leaves block unchanged; accept is one undo step.
