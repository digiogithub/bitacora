---
id: BIT-T-0128
type: task
title: Span-based page-ref and tag rewriter for block content
status: done
parent: BIT-US-0082
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-markdown, rename]
estimate: 3
created: 2026-10-06T14:29:56Z
updated: 2026-10-06T19:30:10Z
closed: 2026-10-06T19:30:10Z
---

## Description
`crates/bitacora-core/src/lifecycle/ref_rewrite.rs`: `fn rewrite_block_refs(content: &str, spans: &InlineSpans, old: &PageName, new: &str) -> Option<String>`. Use the inline scanner spans from `bitacora-markdown` (refs, tags, nested links, labelled `[x]([[Old]])`, `{{embed [[Old]]}}`) so code spans/fences are skipped. Rules:
- `[[Old]]` → `[[New]]` (replace-page-ref!, `page.cljs:229-255`); namespace prefix `[[Old/x]]` → `[[New/x]]`; nested `[[a [[Old]]]]` inner ref rewritten.
- `#Old` → `#New`, or `#[[New Name]]` when new name contains whitespace; `#[[Old]]` → `#[[New]]` (`:257-270`, case-insensitive).
- Matching by page key (case-insensitive, NFC) for both forms (decision for open question 2; record in [[01-file-graph-layout]]).
Return `None` when nothing changes. Edit bytes by span replacement from the end to keep offsets valid.

## Acceptance Criteria
- Table-driven tests: `see [[Old]] and #Old` → `see [[New Idea]] and #[[New Idea]]`; `` `[[Old]]` `` unchanged; `#Older` unchanged; `#old.` → `#New.`; `[[Old/x]]` → `[[New/x]]`; `{{embed [[Old]]}}` rewritten.

## Notes
BIT-SP-0002.R13; BIT-SP-0001.R8 (ref recognition).
