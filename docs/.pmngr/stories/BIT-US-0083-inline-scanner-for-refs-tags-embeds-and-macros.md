---
id: BIT-US-0083
type: story
title: Inline scanner for refs, tags, embeds and macros
status: done
priority: critical
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, parser, refs]
estimate: 8
created: 2026-10-06T14:29:34Z
updated: 2026-10-06T17:23:44Z
closed: 2026-10-06T17:23:44Z
---

## Description
As a Bitacora user, I want every `[[page]]`, `#tag`, `((block-ref))`, embed and macro in my notes recognised exactly like Logseq, so that backlinks, the graph of references and rename cascades are complete and never pick up false refs from code.

"Layer 3" in [[02-markdown-block-syntax]] §9: a small custom inline scanner producing tokens with byte spans for refs, tags, labelled refs, block refs, macros (incl. embeds), inline code and math (which suppress refs), and links to assets/draws. Opaque Markdown constructs (lists, tables, quotes, HTML, `#+BEGIN_*`) stay body text and are rendered later with comrak/pulldown-cmark.

## Acceptance Criteria
- All forms of §5.1 recognised, including `[[a [[b]] c]]` (refs `a [[b]] c` and `b`) and `#[[nested [[tag]]]]`.
- `#tag.` → `tag`, `#foo:` → `foo`; tag ends at whitespace or `, ; . ! ? ' " :`.
- `` `[[x]]` ``, `\[[x]]`, refs in fences and in `#+BEGIN_QUERY` produce no ref.
- `{{embed [[page]]}}` → page ref; `{{embed ((uuid))}}` → block ref; `{{query (and [[a]] (task TODO))}}` captured as macro.
- `[[assets/x.pdf]]` and `[[draws/x.excalidraw]]` are links, not page refs.
- Namespace parents added for `[[a/b/c]]` (`a`, `a/b`).

## Notes
Implements: BIT-SP-0001.R8, BIT-SP-0001.R18.
See [[02-markdown-block-syntax]] §5.1, §5.2, §6, §8; [[03-parsing-indexing-search]] (ref walk `with-page-refs`, `block.cljs:337-372`); ADR-003.
