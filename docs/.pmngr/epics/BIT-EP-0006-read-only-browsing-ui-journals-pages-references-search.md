---
id: BIT-EP-0006
type: epic
title: "Read-only browsing UI: journals, pages, references, search"
status: backlog
priority: high
milestone: BIT-M-0002
author: mcp
labels: [ui]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
`bitacora-app` views on top of core + index: graph picker, left sidebar (journals, all pages, favorites, recent), journals infinite scroll, page view rendering blocks (refs, tags, task markers, properties, code, images, collapsed state), navigation history, linked/unlinked references panel, search/command palette, right sidebar.

## Acceptance Criteria
- All fixture pages render without panics; refs and tags are clickable.
- Linked references for a page match Logseq's for the fixture graph.
- Keyboard-only navigation for search and page switching.

## Notes
See [[04-editor-outliner-operations]] (rendering), [[gpui-and-gpui-kit]] §2.
