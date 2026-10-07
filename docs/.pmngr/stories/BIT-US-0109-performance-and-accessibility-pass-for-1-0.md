---
id: BIT-US-0109
type: story
title: Performance and accessibility pass for 1.0
status: in_review
priority: medium
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [performance, accessibility, bitacora-app, bitacora-index]
estimate: 5
created: 2026-10-06T14:31:47Z
updated: 2026-10-07T00:10:21Z
started: 2026-10-06T22:47:18Z
---

## Description
As a user with a large graph or assistive needs, I want Bitacora to stay fast on big graphs and be usable with keyboard and screen readers, so that 1.0 is dependable for everyone.

## Acceptance Criteria
- Benchmarks on the `large` generated graph (500k blocks): cold build < 60 s, search p95 < 100 ms, page open < 100 ms, query `(and [[a]] [[b]] [[c]])` < 200 ms; decision recorded whether to materialize `block_path_refs`.
- Startup to first journal render < 1 s on a warm index (medium graph).
- Keyboard focus visible everywhere; all actions reachable by keyboard; contrast ≥ WCAG AA in bundled themes; accessibility labels on interactive elements where GPUI exposes them.

## Notes
Implements: BIT-SP-0003.R14, BIT-SP-0003.R16 (MAY materialize path-refs, [[sqlite-index-schema]] §3.1, §9 item 17). [[gpui-and-gpui-kit]] §1.10 (known limitations).
