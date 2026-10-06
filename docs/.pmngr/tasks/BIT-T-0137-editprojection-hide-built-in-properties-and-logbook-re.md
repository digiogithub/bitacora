---
id: BIT-T-0137
type: task
title: "EditProjection: hide built-in properties and LOGBOOK, re-insert at anchors"
status: backlog
priority: critical
parent: BIT-US-0030
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, editor]
estimate: 2
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T14:30:00Z
---

## Description
In `crates/bitacora-core/src/edit/projection.rs` implement `EditProjection::from_text(text, &HiddenKeys)` and `to_text(&self, edited_visible)`. `HiddenKeys` = built-in list (`id custom-id collapsed heading(bool) background-color created-at updated-at last-modified-at query-table query-properties query-sort-by query-sort-desc logseq.order-list-type ls-type hl-* logseq.macro-*`) + config `:block-hidden-properties`. Hidden lines and the `:LOGBOOK:`…`:END:` drawer are stored with the index of the preceding visible line; re-insertion clamps anchors when lines are deleted. `heading:: true` hidden only when boolean.

## Acceptance Criteria
- `to_text(visible)` with unchanged visible returns the original text byte-for-byte (property test over fixtures).
- Editing the title keeps `id::`/`collapsed::` at their positions; deleting all visible lines keeps hidden lines after the first line.
- `heading:: 2` stays visible; `heading:: true` is hidden.

## Notes
Story BIT-US-0030. Implements BIT-SP-0004.R2. Logseq refs: `editor/property.cljs:67-69`, `graph_parser/property.cljs:68-79`.
