---
id: BIT-T-0073
type: task
title: Search latency benchmark on the 5,000-page graph (p95 < 50 ms)
status: done
priority: high
parent: BIT-US-0009
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, benchmark, search, performance]
estimate: 2
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:35:32Z
started: 2026-10-06T18:26:43Z
closed: 2026-10-06T18:35:32Z
---

## Description
`crates/bitacora-index/benches/search.rs`: using the `medium` generated graph (see synthetic generator task), run 1,000 seeded random queries (1–3 words drawn from the corpus vocabulary, 10% 2-char, 5% CJK, 10% with typos) through the full `search()` pipeline incl. fuzzy + RRF + snippets for the top 20. Report p50/p95/p99.

## Acceptance Criteria
- p95 < 50 ms on the reference CI runner; result posted to the bench job summary.
- Also reports the cost split per stage to guide tuning.

## Notes
BIT-SP-0003.R14. Epic acceptance criterion of BIT-EP-0005.
