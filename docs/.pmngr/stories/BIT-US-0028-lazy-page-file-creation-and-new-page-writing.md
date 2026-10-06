---
id: BIT-US-0028
type: story
title: Lazy page file creation and new-page writing
status: done
priority: critical
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, lifecycle]
estimate: 5
created: 2026-10-06T14:28:02Z
updated: 2026-10-06T19:13:32Z
closed: 2026-10-06T19:13:32Z
---

## Description
As a user, I want new pages to get a file only once I write real content, with the same path, name and page-property format Logseq uses, so that navigating or referencing pages never litters my graph and Logseq reads Bitacora-created pages identically.

Pages that are only referenced (`[[New Page]]`), alias pages, namespace parents and property pages live in the model/index only. The file path is assigned lazily on the first save with non-blank content (`modules/file/core.cljs:115-166`): `<:pages-directory>/<encoded title>.md`, encoded with the active `:file/name-format`. In legacy graphs, `title::` is written when the title does not survive an encode→decode round-trip (`fs.cljs:198-206`). New page properties are written as an un-bulleted pre-block followed by one blank line; front matter is never created.

## Acceptance Criteria
- Navigating to `[[New Page]]` and leaving without typing creates no file.
- Typing `hello` into the first block of `Projects/Bitacora` (triple-lowbar) creates `pages/Projects___Bitacora.md` with content `- hello`; no file for `Projects`.
- In a legacy graph, creating `Version 1.0` with `- hello` writes `title:: Version 1.0\n\n- hello`.
- A page backed by `pages/Notes.org` never gets a `.md` twin.
- Adding page property `tags:: demo` to a page `- a` yields `tags:: demo\n\n- a`; in a front-matter page it is written as `tags: demo` inside the front matter.
- File name keeps the case of the original title; output is UTF-8, LF, no BOM.

## Notes
Implements: BIT-SP-0002.R12, BIT-SP-0002.R7, BIT-SP-0002.R18, BIT-SP-0002.R16
See [[01-file-graph-layout]] §3.3, §8, §9, §10.1; [[04-editor-outliner-operations]]; [[block-editor]]. ADR-011 (atomic writes), ADR-013 (naming formats).
