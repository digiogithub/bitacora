---
id: BIT-T-0049
type: task
title: DockArea with center and right panels and persisted layout
status: backlog
priority: high
parent: BIT-US-0025
milestone: BIT-M-0001
author: mcp
labels: [ui, bitacora-app]
estimate: 3
created: 2026-10-06T14:28:06Z
updated: 2026-10-06T14:28:06Z
---

## Description
Use GPUI Kit `DockArea` in the `Workspace` view: a center panel (placeholder `PageHost` showing "No page open") and a right dock panel (placeholder "Right sidebar", the future Logseq shift-click target). Persist the dock layout JSON (GPUI Kit's dock serialization) to `<data_dir>/workspace.json` on change (debounced 500 ms) and restore on startup; fall back to the default layout if the file is missing or invalid (log a warning, never crash). Actions: `ToggleRightSidebar`.

## Acceptance Criteria
- Resizing/closing the right panel survives an app restart.
- Corrupting `workspace.json` results in the default layout and a warning log line.
- Unit test for the load/fallback function.

## Notes
- [[gpui-and-gpui-kit]] §2.2 (Dock: panels, splits, JSON persistence; Resizable). [[crate-stack]] §4.1 (`serde`/`serde_json` for Dock layout persistence).
