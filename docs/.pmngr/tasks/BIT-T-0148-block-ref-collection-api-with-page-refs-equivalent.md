---
id: BIT-T-0148
type: task
title: Block ref collection API (with-page-refs equivalent)
status: backlog
parent: BIT-US-0083
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, refs]
estimate: 2
created: 2026-10-06T14:30:15Z
updated: 2026-10-06T14:40:05Z
---

## Description
Add `crates/bitacora-markdown/src/refs.rs`: `fn block_refs(block: &ParsedBlock, cfg: &PropertyConfig) -> BlockRefs { pages: BTreeSet<String>, blocks: BTreeSet<Uuid>, tags: BTreeSet<String> }` combining inline tokens from title + body (skipping fences, `#+BEGIN_QUERY`, quotes as opaque regions), refs from property values, marker and priority as refs (`TODO`, `A`), and namespace parents (`a/b/c` → `a`, `a/b`), re-implementing the documented behaviour of `with-page-refs` (`block.cljs:337-372`, reference only). Names are returned with original case; lower-case/NFC keying is done in bitacora-core.

## Acceptance Criteria
- Our own vectors for refs via content, properties and page properties, verified black-box against Logseq (no Logseq test file copied or translated).
- `#+BEGIN_QUERY` with EDN containing `[[x]]` and `"` (fixture 15) yields no ref.
- `- TODO [#A] see [[a/b/c]]` → pages `{TODO, A, a/b/c, a, a/b}`.

## Notes
Part of BIT-US-0083. Implements BIT-SP-0001.R8, R18. Consumed by bitacora-index (see [[sqlite-index-schema]]). ADR-015.
