---
id: BIT-T-0095
type: task
title: "Page property assembly and #+key directive hoisting"
status: backlog
parent: BIT-US-0059
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, properties]
estimate: 2
created: 2026-10-06T14:29:18Z
updated: 2026-10-06T14:29:18Z
---

## Description
Implement `crates/bitacora-markdown/src/page_props.rs`: `fn page_properties(doc: &ParsedPage) -> PageProps`:
- If the first element is front matter or a pre-block property group (`extract.cljc:227-241`), its entries are page properties (keys lower-cased).
- Collect every `#+key: value` directive anywhere in the file (pre-block or any block body, outside fences) and merge them into page properties, as `collect-page-properties` does (`mldoc.cljc:117-131`).
- If the pre-block's properties contain `heading`, it is not a pre-block (`block.cljs:532`).
- Strip a leading U+FEFF before matching keys.
Expose `title`, `alias`, `tags`, `public`, `icon`, `filters` accessors (values interpreted by the property value module).

## Acceptance Criteria
- Fixture 5 of §11: front matter page, `title::` page and `#+title:` inside a nested block each produce the expected title.
- `"title:: My Page\nalias:: Mine, [[My page alias]]\ntags:: project, [[multi word]]\n\n- first block"` → title, alias set and tags set as in BIT-SP-0001.R3.
- BOM page `\u{FEFF}title:: Bom\n\n- a` → title `Bom`.

## Notes
Part of BIT-US-0059. Implements BIT-SP-0001.R3, R19.
