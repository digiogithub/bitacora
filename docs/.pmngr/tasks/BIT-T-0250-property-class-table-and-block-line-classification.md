---
id: BIT-T-0250
type: task
title: Property class table and block line classification
status: backlog
parent: BIT-US-0094
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, merge]
estimate: 2
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T14:32:01Z
---

## Description
Add `crates/bitacora-markdown/src/classify.rs` with a static table built from [[02-markdown-block-syntax]] §5.4 (`property.cljs:46-103`, `srs.cljs:48-53`, `pdf/assets.cljs:131-201`): `fn class_of(key_norm) -> PropClass { Content, Metadata, Identity }`. Metadata: `collapsed`, `card-last-interval`, `card-repeats`, `card-last-reviewed`, `card-next-schedule`, `card-ease-factor`, `card-last-score`, `query-table`, `query-properties`, `query-sort-by`, `query-sort-desc`, `filters`, `ls-type`, `hl-type`, `hl-page`, `hl-stamp`, `hl-color`, `created-at`, `updated-at`, `last-modified-at` (and `_` variants), `todo`, `doing`, `now`, `later`, `done`, `logseq.query/nlp-date`, `exclude-from-graph-view`, `logseq.tldraw.*`. Identity: `id`, `custom-id`, `custom_id`. Everything else Content. Also `classify_lines(block) -> Vec<(Span, LineClass)>` labelling title, body, property lines, SCHEDULED/DEADLINE (Content) and LOGBOOK drawer lines (Metadata).

## Acceptance Criteria
- One unit test per §5.4 row.
- `collapsed:: true`, `card-repeats:: 2`, `id:: …`, `owner:: [[Ana]]`, `:LOGBOOK:` → Metadata, Metadata, Identity, Content, Metadata.

## Notes
Part of BIT-US-0094. Implements BIT-SP-0001.R16. ADR-009.
