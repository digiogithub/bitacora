---
id: BIT-US-0085
type: story
title: Legacy name codec and page title derivation pipeline
status: backlog
priority: critical
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [core, compat]
estimate: 5
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T14:29:54Z
---

## Description
As a user of an older Logseq graph without `:file/name-format`, I want Bitacora to derive every page title exactly as Logseq does (Contents special case, `title::`/front matter, then file name decoded with the active format) so that links resolve to the same pages in both apps.

Adds the legacy codec (`.`→`/` + URL-decode; legacy encoding for new files) and the `create-title-property?` predicate that decides when a legacy page needs `title::`, plus a single `derive_title(path, first_ast_props, cfg)` entry point.

## Acceptance Criteria
- `pages/contents.md` → `Contents`; `pages/foo.md` with `title:: Bar` → `Bar`; front matter `title: Front` honoured.
- Triple-lowbar graph: `pages/Version 1.0.md` → `Version 1.0`; legacy graph: same file → `Version 1/0`.
- Legacy encode `Projects/Bitacora` → `Projects%2FBitacora`.
- `needs_title_property("Version 1.0", Legacy) == true`; `needs_title_property("My Page", Legacy) == false`; always false in triple-lowbar mode at creation.
- Derived title is then passed to journal detection.

## Notes
Implements: BIT-SP-0002.R5, BIT-SP-0002.R7
[[01-file-graph-layout]] §3.1, §3.3, §3.4. ADR-013. [[architecture]]
