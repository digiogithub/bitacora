---
id: BIT-T-0179
type: task
title: Graph picker view and recent graphs store
status: in_progress
priority: high
parent: BIT-US-0073
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:28:35Z
started: 2026-10-06T18:28:35Z
---

## Description
`crates/bitacora-app/src/views/graph_picker.rs`: GPUI view with "Open graph folder…" (`cx.prompt_for_paths` with directories only) and a recent-graphs `List` (name, path, last opened) persisted in app settings via `bitacora-config` (`recent_graphs: Vec<RecentGraph>`, max 10). Validate: path exists, readable; warn (GPUI Kit `Alert`) when `logseq/config.edn` is missing. CLI flag `--graph <path>` bypasses the picker. Menu item "Switch graph…".

## Acceptance Criteria
- `#[gpui::test]`: selecting a recent graph emits `OpenGraph(path)`; missing folder entry shows an error and can be removed.
- Recent list ordering is most-recent-first and survives restart.

## Notes
ADR-001. [[gpui-and-gpui-kit]] §2.2.
