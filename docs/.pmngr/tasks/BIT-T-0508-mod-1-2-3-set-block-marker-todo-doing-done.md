---
id: BIT-T-0508
type: task
title: Mod+1/2/3 set block marker TODO/DOING/DONE
status: done
parent: BIT-US-0170
milestone: BIT-M-0006
author: mcp
created: 2026-10-07T19:25:19Z
updated: 2026-10-07T19:25:19Z
---

Actions outliner::SetMarkerTodo/Doing/Done (editor/actions.rs, handlers in editor/view.rs via Cmd::SetMarker through structural()), bound secondary-1/2/3 in BlockEditor and BlockSelection (default.json). Tests ctrl_1_2_3_* cover editing block and selection, undo, exact bytes. Not verified on macOS (cmd-1/2/3 via secondary).
