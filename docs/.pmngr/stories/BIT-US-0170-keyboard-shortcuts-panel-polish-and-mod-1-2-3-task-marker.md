---
id: BIT-US-0170
type: story
title: Keyboard shortcuts panel polish and Mod+1/2/3 task-marker shortcuts
status: in_review
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
created: 2026-10-07T19:25:14Z
updated: 2026-10-07T19:25:30Z
started: 2026-10-07T19:25:14Z
---

## Description
Owner request 2026-10-07. (1) Settings > Keymap panel to view/modify key bindings (list by context, filter, record, conflict warning, reset one/all, persisted in keymap.json, live apply). The panel already existed from BIT-T-0332; this story groups rows by context and adds a record/persist test. (2) Editor actions SetMarkerTodo/Doing/Done bound to secondary-1/2/3 (Ctrl on Linux/Windows, Cmd on macOS) for the editing block and block selection, one undoable core op.

## Acceptance Criteria
- Rows grouped under context headers; record test passes.
- Mod+1/2/3 set TODO/DOING/DONE with exact file bytes and undo.
- every_action_is_reachable_from_the_keyboard stays green.
