---
id: BIT-T-0047
type: task
title: Synthetic graph generator and cold-build benchmark
status: done
priority: high
parent: BIT-US-0007
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, benchmark, performance]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
`crates/bitacora-index/benches/gen.rs` (shared module, also used by search benches): deterministic (seeded) generator of Logseq graphs: N pages + M journals, Zipf-distributed `[[refs]]` and `#tags`, 10% tasks, properties, nesting up to depth 6, 5% CJK text. Presets `medium` (5,000 pages, ~50k blocks) and `large` (~500k blocks). `benches/cold_build.rs`: cold build time, DB size, warm reopen time (no changes).

## Acceptance Criteria
- `cargo bench -p bitacora-index --bench cold_build` reports: medium cold build (target < 5 s), warm reopen (target < 300 ms), DB size.
- CI bench job posts numbers to the job summary; regression > 20% vs main prints a warning.

## Notes
BIT-SP-0003.R16. [[sqlite-index-schema]] §4.6.
