---
id: BIT-T-0014
type: task
title: Author the edge-case fixture graph
status: in_progress
priority: critical
parent: BIT-US-0011
milestone: BIT-M-0001
author: mcp
labels: [infra, fixtures]
estimate: 3
created: 2026-10-06T14:26:00Z
updated: 2026-10-06T16:46:30Z
started: 2026-10-06T16:46:30Z
---

## Description
Create `fixtures/graphs/edge-cases/` with one small file per quirk (named after the quirk) plus `logseq/config.edn` and `PROVENANCE.md` ("hand-authored for Bitacora; each file cites the Logseq source line or analysis section it exercises"). Cover at least:
- Outline: tab vs 2-space vs 4-space indentation, mixed tabs/spaces, irregular indent (`extract_test.cljs:81-87` case), ATX heading without bullet, `heading:: true`, continuation lines, empty blocks, whitespace-only lines, trailing whitespace.
- Pre-block: `key:: value` page properties, YAML front matter, `#+title:` directive deep in a page, file with no bullets.
- Inline: `[[page]]`, `[[nested [[ref]]]]`, `#tag`, `#[[multi word]]`, `((uuid))`, `{{embed}}`, `{{query}}`, escaped `\[[x]]`, quoted property values.
- Tasks: TODO/DOING/NOW/LATER/DONE/CANCELED, priorities, `SCHEDULED:`/`DEADLINE:`, `:LOGBOOK:` drawers, `collapsed:: true`, `id::` properties.
- Encodings: CRLF file, UTF-8 BOM file, non-ASCII (CJK, emoji) titles, unclosed code fence.
- File names: triple-lowbar and legacy encodings of namespaced titles (`a/b`), reserved chars, a journal in `journals/yyyy_MM_dd.md`, a `.org` page, `logseq/bak/` and `logseq/.recycle/` entries, an `assets/` image.
- A second config variant directory (`edge-cases-legacy-names/`) with `:file/name-format` absent (legacy) if one graph cannot cover both.

## Acceptance Criteria
- Every quirk listed above has at least one file, and `PROVENANCE.md` maps file -> quirk -> doc reference.
- Files open in Logseq 0.10.15 without errors (manual check noted in the PR).

## Notes
- [[02-markdown-block-syntax]] §2 and edge-case table; [[01-file-graph-layout]] (naming, journals, bak/recycle). ADR-003, ADR-013.
