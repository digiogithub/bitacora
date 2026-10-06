---
id: BIT-US-0098
type: story
title: "Graph domain model: pages, page properties, aliases and namespaces"
status: done
priority: high
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [core, compat]
estimate: 5
created: 2026-10-06T14:31:16Z
updated: 2026-10-06T17:33:06Z
started: 2026-10-06T17:31:35Z
closed: 2026-10-06T17:33:06Z
---

## Description
As a developer of the index, UI and MCP layers, I want a single in-memory `Graph`/`Page`/`Block` model in `bitacora-core` that already applies Logseq's page semantics (page properties from the pre-block, aliases, tags, namespaces, virtual pages without files) so that every consumer resolves pages the same way.

Pages can be file-backed or virtual (referenced, alias, namespace parent, property page, tag page). Virtual pages never imply a file.

## Acceptance Criteria
- `Page { key, original_name, file: Option<GraphPath>, format, journal: Option<JournalInfo>, props, aliases, tags, namespace_parent, read_only }`.
- `alias:: Mine, [[My page alias]]` on `My Page`: `[[Mine]]` resolves to `My Page`; blank alias and self-alias dropped; no file created.
- `a/b/c` page creates virtual `a` and `a/b` with `namespace_parent` links; titles starting with `./`, `../` or URLs are not namespaces.
- Page properties are read from the pre-block (keys lower-cased); front matter keys honoured.
- Loading a fixture graph and re-saving nothing produces zero writes.

## Notes
Implements: BIT-SP-0002.R18 (read side), BIT-SP-0002.R12 (model-level: no files for virtual pages), BIT-SP-0002.R8
[[01-file-graph-layout]] §3.5, §9; Logseq `extract.cljc:65-103,186-199`. ADR-006, ADR-012. [[architecture]] [[block-editor]]
