---
id: BIT-T-0183
type: task
title: "Block decorations: marker checkbox, priority, timestamps, properties, logbook, headings"
status: done
priority: high
parent: BIT-US-0074
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, rendering]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:44:23Z
started: 2026-10-06T18:28:35Z
closed: 2026-10-06T18:44:23Z
---

## Description
`crates/bitacora-app/src/render/block.rs`: `BlockView` element = bullet (Lucide `Icon`, filled when collapsed with children) + collapse arrow + content + children. Decorations: TODO/DOING/DONE marker as `Checkbox` (read-only in this epic), DONE strikethrough, priority `Badge` (`A`/`B`/`C`), SCHEDULED/DEADLINE `Tag` chips, properties table (`DescriptionList`) hiding hidden built-ins (`id`, `collapsed`, `heading`, ...), collapsed `:LOGBOOK:` summary, heading sizes for `#`/`heading:: N`.

## Acceptance Criteria
- `#[gpui::test]` snapshots of layout trees for marker, priority, properties and heading cases.
- Hidden properties never displayed.

## Notes
[[04-editor-outliner-operations]] §8; [[block-editor]] §8.
