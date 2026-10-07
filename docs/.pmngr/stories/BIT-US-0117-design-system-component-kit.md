---
id: BIT-US-0117
type: story
title: Design system component kit
status: done
priority: high
parent: BIT-EP-0015
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, bitacora-app, components]
estimate: 8
created: 2026-10-07T09:11:26Z
updated: 2026-10-07T11:00:59Z
closed: 2026-10-07T11:00:59Z
---

## Description
As a developer, I want reusable components matching `docs/componentes.md`, so that screens are assembled from consistent parts.

## Acceptance Criteria
- In `crates/bitacora-app/src/ui/components/`: `Button` (primary/secondary/ghost/AI), `IconButton`, `Kbd`, `Chip`, `TaskMarker`, `Pill`, `Overline`, `Card`, `Tab`, `Segmented`, `Popover` shell, Lucide icon set + AI sparkle icon.
- Built on kit primitives through the facade; sizes from `Metrics`; targets >= 28px with focus ring.
- One `#[gpui::test]` per component; `only_ui_names_gpui_kit` passes.

## Notes
Implements BIT-SP-0008.R1, BIT-SP-0008.R6.
