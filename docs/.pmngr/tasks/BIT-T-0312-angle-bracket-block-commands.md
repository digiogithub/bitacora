---
id: BIT-T-0312
type: task
title: Angle-bracket block commands
status: backlog
priority: medium
parent: BIT-US-0105
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, editor]
estimate: 2
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
`crates/bitacora-app/src/editor/commands/angle.rs`: `<` at line start opens a menu: quote, src (asks language), note, tip, important, caution, warning, pinned, example, export, center, comment; inserts `#+BEGIN_X` / `#+END_X` (or Markdown ```` ``` ```` for src, `>` for quote, matching Logseq output) with the caret inside.

## Acceptance Criteria
- `#[gpui::test]`: `<note` inserts `#+BEGIN_NOTE\n\n#+END_NOTE` with caret on the empty line; output byte-identical to Logseq for each command (fixture comparison).

## Notes
[[04-editor-outliner-operations]] Requirements 15.
