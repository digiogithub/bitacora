---
id: BIT-US-0058
type: story
title: Property scanner with mldoc key rules and Logseq value semantics
status: backlog
priority: critical
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, parser, properties]
estimate: 8
created: 2026-10-06T14:28:45Z
updated: 2026-10-06T14:28:45Z
---

## Description
As a Bitacora user, I want `key:: value` properties (and Markdown `:PROPERTIES:` drawers) parsed exactly as Logseq parses them, so that page/block metadata, tags, aliases and property refs show up identically in both apps and nothing I typed is lost.

This is "Layer 2" in [[02-markdown-block-syntax]] §9: a property scanner that recognises property groups with mldoc's key rules, keeps each line's byte span and original text, classifies valid vs invalid keys, normalises keys for lookup, and interprets values (`text.cljs:87-187`).

## Acceptance Criteria
- `a.b.c:: 1` and `empty::` are properties; `my key:: v` and `key::value` are text.
- First property group in a block body wins; groups inside quotes, fences and `#+BEGIN` blocks are ignored.
- `tags:: a, [[b c]], #d` → `{"a","b c","d"}`; `foo:: a, b` → string; `tags:: "foo, bar"` → quoted raw string; `true`/`1000` typed.
- Invalid keys (`"x"::`, `(a)::`) are reported and kept verbatim; order and original text preserved.
- `:PROPERTIES:` drawer in a Markdown block yields properties (with `custom_id` → `id`) without changing bytes.

## Notes
Implements: BIT-SP-0001.R4, BIT-SP-0001.R5, BIT-SP-0001.R6, BIT-SP-0001.R14.
See [[02-markdown-block-syntax]] §3, §5.4; [[03-parsing-indexing-search]]; ADR-003.
