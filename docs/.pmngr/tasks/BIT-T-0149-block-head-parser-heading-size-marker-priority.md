---
id: BIT-T-0149
type: task
title: "Block head parser: heading size, marker, priority"
status: done
parent: BIT-US-0084
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, tasks]
estimate: 2
created: 2026-10-06T14:30:15Z
updated: 2026-10-06T17:23:44Z
closed: 2026-10-06T17:23:44Z
---

## Description
Implement `crates/bitacora-markdown/src/head.rs`: parse the first line of a block after the bullet into `BlockHead { heading_size: Option<u8>, marker: Option<Marker>, priority: Option<char>, title_span }` following the grammar in [[02-markdown-block-syntax]] §2.1: `[ws* hashes] [ws+ marker] [ws+ priority] [ws* title]`. Markers `TODO DOING DONE LATER NOW WAITING WAIT CANCELED CANCELLED STARTED IN-PROGRESS` only when followed by a space (mldoc 1.5.7 `heading0.ml:16-28`); priority `[#` + any single char + `]`. Also provide `Marker::cycle_todo()` / `cycle_now()` (`marker.cljs:40-58`) for the editor.

## Acceptance Criteria
- `- LATER` → no marker; `- LATER read book` → LATER; `- TODOx thing` → none; `- ## TODO [#A] ship it` → size 2, TODO, A.
- `- [#B] only priority` → priority B, no marker.
- Our own marker/priority vectors covering the documented cases, verified black-box against Logseq/mldoc (no Logseq test file copied or translated).

## Notes
Part of BIT-US-0084. Implements BIT-SP-0001.R9, R2. ADR-015.
