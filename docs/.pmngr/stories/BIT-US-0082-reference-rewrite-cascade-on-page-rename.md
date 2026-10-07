---
id: BIT-US-0082
type: story
title: Reference rewrite cascade on page rename
status: done
priority: high
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, rename]
estimate: 8
created: 2026-10-06T14:29:32Z
updated: 2026-10-07T08:21:31Z
started: 2026-10-06T19:30:10Z
closed: 2026-10-07T08:21:31Z
---

## Description
As a user, I want every reference to a renamed page — `[[Old]]`, `#Old`, `#[[Old]]`, `old::` property keys and refs in property values — rewritten across the whole graph, so that no link breaks and the result matches what Logseq would produce.

Logseq's `rename-update-refs!` (`handler/page.cljs:393-423`) rewrites each referring block: `replace-page-ref!` (`:229-255`, case-sensitive on the original name), tag replacement (`:257-270`, case-insensitive, word boundaries, `#[[New Name]]` when the new name has whitespace), property keys (`:272-278`) and values (`:289-311`). Unlike Logseq, Bitacora rewrites only the affected blocks (byte-preserving serializer) instead of re-serializing whole files. Open question 2 (case-sensitive `[[...]]` replace) must be decided and documented.

## Acceptance Criteria
- `see [[Old]] and #Old` → `see [[New Idea]] and #[[New Idea]]` after `Old` → `New Idea`.
- `old:: value` → `new-idea:: value`; `tags:: Old, x` → `tags:: New Idea, x` (or `[[New Idea]]`, matching Logseq output).
- `[[old]]` (different case) handling follows the documented decision and is covered by a test.
- Namespace refs `[[a/x]]` → `[[b/x]]` when `a` → `b`.
- Refs inside inline code / fences are not touched.
- Golden tests: rename cascade on `fixtures/graphs/rename` yields the same block text as Logseq 0.10.15 (expected outputs captured once from Logseq and committed); untouched blocks byte-identical.

## Notes
Implements: BIT-SP-0002.R13
See [[01-file-graph-layout]] §10.2, open question 2; [[02-markdown-block-syntax]] §5.1; [[04-editor-outliner-operations]]; [[block-editor]]. ADR-011.
