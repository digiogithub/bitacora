---
id: BIT-T-0195
type: task
title: Actions palette and global navigation keybindings
status: done
priority: medium
parent: BIT-US-0078
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:52:50Z
started: 2026-10-06T19:14:39Z
closed: 2026-10-06T19:52:50Z
---

## Description
`crates/bitacora-app/src/actions.rs`: register GPUI actions (`GoToJournals` `g j`, `GoToAllPages` `g a`, `ToggleLeftSidebar` `t l`, `ToggleRightSidebar` `t r`, `OpenSearch` Mod+K, `OpenActions` Mod+Shift+P, `Reindex`) in a `Workspace` key context; Mod+Shift+P lists them in a `Command` palette with their `Kbd` shortcuts, fuzzy-filtered with `nucleo`.

## Acceptance Criteria
- `#[gpui::test]`: each binding dispatches its action; palette lists every registered action.
- Keyboard-only flow documented in the PR (open graph → search → page → back).

## Notes
[[04-editor-outliner-operations]] §7 (default shortcuts).
