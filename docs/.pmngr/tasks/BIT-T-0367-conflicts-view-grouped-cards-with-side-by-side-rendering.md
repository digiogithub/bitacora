---
id: BIT-T-0367
type: task
title: "Conflicts view: grouped cards with side-by-side rendering and word diff"
status: done
priority: high
parent: BIT-US-0054
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, merge, ui]
estimate: 3
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T20:37:17Z
closed: 2026-10-06T20:37:17Z
---

## Description
`crates/bitacora-app/src/views/conflicts/mod.rs`: GPUI Kit view listing `ConflictRecord`s grouped by page (virtualized), card shows breadcrumb, kind badge, theirs author/time, ours | theirs columns rendered with the block renderer, word-level diff vs base (`similar` crate word diff) highlighted. Variants for delete-vs-modify, rename_rename (title choice + alias suggestion), config (whole-key choice), text/binary (side-by-side text or file names). "Conflicts (N)" banner on page header; conflicted blocks get a gutter marker linking to their card.

## Acceptance Criteria
- `#[gpui::test]` for grouping, counts and variant selection.

## Notes
Story BIT-US-0054. Implements BIT-SP-0006.R16. ADR-001, ADR-002.
