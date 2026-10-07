---
id: BIT-EP-0009
type: epic
title: "Page lifecycle: create, rename, delete, aliases and assets"
status: done
priority: high
milestone: BIT-M-0003
author: mcp
labels: [core, compat]
created: 2026-10-06T14:21:13Z
updated: 2026-10-07T08:21:43Z
started: 2026-10-07T00:15:15Z
closed: 2026-10-07T08:21:43Z
---

## Description
Lazy page file creation (only on first non-blank content), journal auto-creation for today (virtual until edited), page rename with Logseq's full reference-rewrite cascade (`[[Old]]`, `#Old`, `old::`), page merge on rename collision, delete to `logseq/.recycle/`, aliases, namespaces, assets paste/drop with Logseq naming (`assets/<stem>_<epoch-ms>_<i><ext>`, `../assets/` links), legacy `title::` auto-write.

## Acceptance Criteria
- Rename cascade produces byte-identical results to Logseq on the fixture graph.
- No empty page files are created by navigation or references.

## Notes
See [[01-file-graph-layout]].
