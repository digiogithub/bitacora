---
id: BIT-US-0169
type: story
title: "Special blocks: code, quote and admonition rendering and editing"
status: backlog
priority: medium
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, editor, bitacora-app]
created: 2026-10-07T19:20:02Z
updated: 2026-10-07T19:20:02Z
---

## Description
Logseq 0.10.x special blocks: fenced and #+BEGIN_SRC code, #+BEGIN_QUOTE and `> ` quotes, admonitions (NOTE, TIP, IMPORTANT, CAUTION, WARNING, PINNED), EXAMPLE, CENTER, VERSE, COMMENT. Slash/angle commands insert Logseq's exact markers; read mode renders them styled with theme tokens; edit mode shows raw markers, Enter inside a code region inserts a newline, Tab indents inside code.

## Acceptance Criteria
- Parser/serializer stay lossless for each kind (fixture round-trip).
- `<src` in Markdown inserts a fence (Logseq ->block behaviour); other `<` commands insert `#+BEGIN_X`.
- Read mode: code with language label + copy button, quote with left border, admonition callouts (icon + title per kind), example/verse/center.
- Enter inside a code region does not split the block; Tab inserts indentation.
