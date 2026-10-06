---
created_at: 2026-10-06T17:23:58.010780461Z
updated_at: 2026-10-06T17:23:58.010780461Z
tags:
    - change
    - markdown
    - parser
---
# BIT-US-0083 + BIT-US-0084 + BIT-US-0059: inline scanner, task syntax, page properties

Part of [[bitacora-full-development-plan]]; builds on [[bit-us-0026-bit-us-0058-outline-splitter-and-property-scanner]]. Designs: [[02-markdown-block-syntax]] (new section 5.3.1 lists every observed mldoc 1.5.7 rule).

## What changed (crate bitacora-markdown, commit b4d1008)
- `src/inline/{scan,refs}.rs`: `scan`, `scan_line`, `InlineToken` (PageRef w/ nested, Tag, BlockRef, Link, Macro, Code, Math, Html, Url), emphasis-restricted scanning, `collect`, `RefSet`, `RefMode`, `namespace_parents`, `is_asset_or_draw`.
- `src/properties/value.rs`: `scan_refs` now uses the real scanner (property-value mode: top-level refs only, tags mid-word, macro args scanned).
- `src/tasks/{head,timestamp,drawer}.rs`: `parse_head`/`Marker`, `parse_timestamp`/`Timestamp`/`Repeater`, `parse_planning_line`, `find_drawers`, `parse_clock`, `parse_logbook`, `format_clock`, durations.
- `src/block.rs`: `analyze` (head, planning lift, drawers, logbook, properties, `BlockRefs`).
- `src/page_props.rs`: `page_properties` (pre-block, YAML front matter, `#+key` hoisting, heading-not-pre-block, BOM).
- Oracles `tools/mldoc-diff/{inline,tasks,pageprops}.js`; fixtures `fixtures/markdown/{inline,tasks,page-props}/`; tests `tests/{inline_fixtures,tasks_fixtures,page_props_fixtures,inline_robustness}.rs`.

## Findings (mldoc behaviour that differs from the earlier docs)
Emphasis hides tags/block refs; tags can start mid-word; directives (front matter or `#+`) replace pre-block page properties; front matter needs every line `key: value`; marker needs a following space; stray `:END:` swallows bullets (not replicated).

## Known divergences
Inline HTML without a matching close (`<br> [[a]] <br/>`), `+1w 10:30` repeater/time ordering, `#[[a]]b` tag name; embed of non-ref text adds a page (as Logseq code) but `((uuid))` embeds only a block ref.

## Verification
fmt, clippy -D warnings clean; `cargo test -p bitacora-markdown --locked`: 114 unit + 9 integration tests pass (incl. ~240 inline cases, 3 task fixture files, 41 page-prop cases vs mldoc, proptest no-panic); check-deps OK.
