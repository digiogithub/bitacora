---
id: BIT-T-0237
type: task
title: "Page properties from pre-block: title, alias, tags, public, filters, icon"
status: backlog
priority: high
parent: BIT-US-0098
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:31:34Z
updated: 2026-10-06T14:31:34Z
---

## Description
`crates/bitacora-core/src/model/page_props.rs`: build `PageProps` from the markdown crate's pre-block properties (or front matter / hoisted `#+key:` directives), port of `extract.cljc:186-193,227-242`:
- keys lower-cased; invalid keys ignored (kept in text);
- `title` raw string; `alias`/`aliases`/`tags` comma-split sets of page names (uses BIT-SP-0001 value semantics); `public` bool; `filters` EDN map string with backslashes stripped (`extract.cljc:238-242`); `icon`, `exclude-from-graph-view`, `template` passthrough;
- remember `source: PreBlock | FrontMatter` for later writes (R18 write side lives in EP-0009).

## Acceptance Criteria
- Tests: `title:: My Page\nalias:: Mine, [[My page alias]]\ntags:: project, [[multi word]]` → expected struct; front matter `---\ntitle: Front\ntags: a\n---`; `filters:: {"tag" true, "other" false}` parsed to map.

## Notes
Refs BIT-SP-0002.R18, BIT-SP-0002.R5.
