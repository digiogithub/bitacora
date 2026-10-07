---
id: BIT-US-0165
type: story
title: Reopen the last graph at startup and open graphs from a menu
status: backlog
priority: high
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ux, bitacora-app]
estimate: 3
created: 2026-10-07T09:55:25Z
updated: 2026-10-07T09:55:25Z
---

## Description
As a user, I want Bitacora to open the graph I used last when it starts, instead of asking for a folder every time, and to open another graph from a menu.

## Acceptance Criteria
- At startup without `--graph`, the app opens the most recent graph stored in the user profile (the existing recents list in `recent_graphs_file()`, or an explicit `last_graph` setting). The picker is shown only when there is no recent graph or the path no longer exists; in that case the user gets a message and the entry is pruned or flagged.
- A setting "Reopen last graph on startup" is on by default.
- A graph menu offers "Open graph…" (folder dialog), "Open recent ▸" (recents list) and "Close graph". It is reachable from the app menu/top bar (frameless: in-window menu on Linux/Windows; native File menu on macOS), from the sidebar footer graph switcher and from the palette (`SwitchGraph`, existing).
- Second-instance launches with `--graph` keep working (BIT-T-0153 path).
- `#[gpui::test]`:
  - startup with a stored recent opens it without the picker;
  - a missing path falls back to the picker;
  - the menu actions dispatch `open_graph`.

## Notes
Owner request 2026-10-07. Current code:
- `crates/bitacora-app/src/app.rs` `open_workspace` only opens `args.graph`;
- `views/workspace.rs` builds `GraphPicker` from `RecentGraphs` (`recent.rs`);
- `paths.rs:100` `recent_graphs_file()`;
- `ui/mod.rs:275` `set_menus`.

Can be delivered before the rest of v2.
