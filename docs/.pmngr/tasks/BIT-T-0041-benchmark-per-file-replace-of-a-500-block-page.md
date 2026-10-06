---
id: BIT-T-0041
type: task
title: Benchmark per-file replace of a 500-block page
status: done
priority: medium
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, benchmark, performance]
estimate: 1
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
`crates/bitacora-index/benches/replace.rs` (criterion): pre-index a 5,000-page generated graph, then measure `replace_file` of a 500-block page with 3 refs per block, and `delete_file` of the same page.

## Acceptance Criteria
- Median replace < 10 ms on the reference CI runner; result printed in the CI bench job summary.
- Bench runs with `cargo bench -p bitacora-index --bench replace`.

## Notes
BIT-SP-0003.R5. [[sqlite-index-schema]] §4.6.
