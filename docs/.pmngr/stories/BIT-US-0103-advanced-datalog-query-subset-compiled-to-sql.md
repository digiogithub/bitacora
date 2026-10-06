---
id: BIT-US-0103
type: story
title: Advanced Datalog query subset compiled to SQL
status: backlog
priority: medium
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [query, bitacora-index]
estimate: 8
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T14:37:38Z
---

## Description
As a power user, I want common advanced queries (`#+BEGIN_QUERY` with `:query [:find ...]`) to work, and a clear message for the parts that don't, so that I can migrate my Logseq graph without silent breakage.

## Acceptance Criteria
- EDN reader for `#+BEGIN_QUERY ... #+END_QUERY` maps (`:title`, `:query`, `:inputs`, `:collapsed?`, `:result-transform`, `:view`).
- Conjunctive query compiler over the attribute relations of [[sqlite-index-schema]] §8, with predicates, `not`/`or`/`-join`, inputs, `pull` and aggregates.
- Unsupported constructs produce `unsupported: <construct>` and raw results where possible.
- A corpus of ≥ 25 realistic advanced queries is classified supported/unsupported and the supported ones match Logseq results (Logseq used as a black-box oracle). Queries are written by us to cover the patterns seen in Logseq docs and public graphs; a query is included verbatim only if its source license allows redistribution.

## Notes
Implements: BIT-SP-0003.R19. [[sqlite-index-schema]] §8; [[03-parsing-indexing-search]] §8.2. Open question 7 in [[sqlite-index-schema]]. ADR-015 (corpus is our own or redistributable; Logseq only as black-box oracle).
