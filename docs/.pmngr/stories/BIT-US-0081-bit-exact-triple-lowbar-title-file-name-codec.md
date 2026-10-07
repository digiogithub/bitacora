---
id: BIT-US-0081
type: story
title: Bit-exact :triple-lowbar title ↔ file name codec
status: done
priority: critical
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [core, compat]
estimate: 5
created: 2026-10-06T14:29:27Z
updated: 2026-10-07T08:21:32Z
started: 2026-10-06T16:54:06Z
closed: 2026-10-07T08:21:32Z
---

## Description
As a user sharing a graph between Logseq and Bitacora, I want page titles to map to exactly the same file names in both apps, so that a page created in one app is found by the other and no duplicate files appear.

Re-implement the triple-lowbar title ↔ file name codec in Rust in `bitacora-core` from the documented behaviour in [[01-file-graph-layout]] §3.2 (Logseq reference for behaviour only: `tri-lb-file-name-sanity`, `src/main/frontend/util/fs.cljs:76-135`; `tri-lb-title-parsing`, `deps/graph-parser/src/logseq/graph_parser/util.cljs:10-22,144-160`). Verify with our own vectors covering the documented cases, checked black-box against Logseq.

## Acceptance Criteria
- All §3.2 examples pass (e.g. `Projects/Bitacora/Design` ↔ `Projects___Bitacora___Design`, `What? A: B` ↔ `What%3F A%3A B`, `a%2Fb` → `a%252Fb`, `foo___bar` → `foo%5F%5F%5Fbar`, `.hidden` → `%2Ehidden`, `CON` → `CON___`, `ends with.` → `ends with.___`).
- Our own vector set covers the edge cases Logseq's naming tests exercise (reserved chars with `/`, `__/` sequences, percent-escapes, leading `.`, Windows reserved names), with expected outputs confirmed black-box against Logseq 0.10.15.
- Property test: `decode(encode(t)) == nfc(t)` for titles without empty namespace segments or boundary `/`.

## Notes
Implements: BIT-SP-0002.R6
ADR-013. ADR-015 (re-implement from docs; no Logseq code or test files copied/translated). [[01-file-graph-layout]] §3.2. [[architecture]]
