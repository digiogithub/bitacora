---
id: BIT-T-0386
type: task
title: Kbd, Chip, TaskMarker, Pill and Overline components
status: done
priority: high
parent: BIT-US-0117
milestone: BIT-M-0006
author: mcp
labels: [v2, components]
estimate: 2
created: 2026-10-07T09:12:06Z
updated: 2026-10-07T11:00:59Z
closed: 2026-10-07T11:00:59Z
---

## Description
Small inline components from `docs/componentes.md`: keyboard hint, tag/task chips, TODO/DOING/DONE/LATER/NOW markers in mono, filter pills with counts, uppercase overline labels.

## Acceptance Criteria
- `#[gpui::test]` per component; markers map every Logseq task marker.
