---
id: BIT-T-0101
type: task
title: "Rewrite title:: and front-matter title on rename"
status: backlog
parent: BIT-US-0061
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, rename]
estimate: 2
created: 2026-10-06T14:29:22Z
updated: 2026-10-06T14:29:22Z
---

## Description
In `rename.rs`, if the page's first (properties) block, lower-cased, contains the old lower-cased name (`page.cljs:460-471`), rewrite its `title` property: `title:: New` for `k:: v` pre-blocks, `title: New` in YAML front matter (`util/property.cljs:222-224`). Only the property line changes. Decision for open question 7: also rewrite a stale `title::` whose value equals the old name case-insensitively (document in [[01-file-graph-layout]] Open questions).

## Acceptance Criteria
- `title:: foo\n\n- a` → `title:: bar\n\n- a`.
- `---\ntitle: foo\n---` → `---\ntitle: bar\n---`.
- Page without `title::` gets no `title::` (triple-lowbar).
- Golden tests in `crates/bitacora-core/tests/rename_title.rs`.

## Notes
[[01-file-graph-layout]] §3.4. BIT-SP-0002.R13.
