---
id: BIT-T-0144
type: task
title: "derive_title: Contents, title:: / front matter, then decoded file body"
status: done
priority: critical
parent: BIT-US-0085
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:30:11Z
updated: 2026-10-06T16:58:59Z
closed: 2026-10-06T16:58:59Z
---

## Description
`crates/bitacora-core/src/graph/title.rs`: `derive_title(path: &GraphPath, page_props: Option<&PageProps>, codec: &dyn NameCodec) -> DerivedTitle { original_name, source: Contents|Property|FileName }`, port of `get-page-name` (`extract.cljc:30-63`):
1. path starts with `pages/contents.` → `Contents` (hard-coded `pages/`, not `:pages-directory`).
2. `title` key (case-insensitive) of the first AST element if it is a property pre-block or front matter (from `bitacora-markdown` pre-block API). Value verbatim — quotes/commas not special for `title` (`page.cljs:67-71`).
3. Only for `md`/`markdown`/`org`: `codec.decode(path.file_body())`.
Sub-directories of `pages/` do not contribute to the title.

## Acceptance Criteria
- Tests for all three scenarios of BIT-SP-0002.R5 plus `pages/sub/x.md` → `x`, `title:: "Quoted, Title"` → `"Quoted, Title"` verbatim.
- Mismatch title vs file name produces no warning.

## Notes
Refs BIT-SP-0002.R5. Journal conversion happens in the journals story.
