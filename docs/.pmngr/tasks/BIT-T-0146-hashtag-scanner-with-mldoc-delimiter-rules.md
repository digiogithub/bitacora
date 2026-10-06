---
id: BIT-T-0146
type: task
title: Hashtag scanner with mldoc delimiter rules
status: done
parent: BIT-US-0083
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, refs]
estimate: 2
created: 2026-10-06T14:30:15Z
updated: 2026-10-06T17:23:44Z
closed: 2026-10-06T17:23:44Z
---

## Description
Add `crates/bitacora-markdown/src/inline/tags.rs`: recognise `#tag` and `#[[multi word]]` (including `#[[nested [[tag]]]]`) per mldoc `extended/hash_tag.ml:5-30`: a tag starts at `#` preceded by start/whitespace, runs until whitespace or one of `, ; . ! ? ' " :` and excludes trailing punctuation; `#` + space is not a tag; at column 0 of a top-level line `# x` is a heading (handled by the line scanner), `#x` is a tag. Validate with `tag-valid?` (`deps/graph-parser/src/logseq/graph_parser/util.cljs:61-64`): no `#`, spaces or newlines in plain tags. Emit `Tag { name, span, bracketed }`.

## Acceptance Criteria
- `see #tag. and #foo: and #[[a b]]` → `tag`, `foo`, `a b`.
- `#[[nested [[tag]]]]` → `nested [[tag]]` plus nested ref `tag`.
- `a#b` (no preceding whitespace) behaviour matches mldoc (fixture recorded via the mldoc differential script).
- Tags inside inline code produce nothing.

## Notes
Part of BIT-US-0083. Implements BIT-SP-0001.R8.
