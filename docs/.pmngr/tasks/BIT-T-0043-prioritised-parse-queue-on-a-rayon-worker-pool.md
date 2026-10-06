---
id: BIT-T-0043
type: task
title: Prioritised parse queue on a rayon worker pool
status: backlog
priority: critical
parent: BIT-US-0007
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index, performance]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T14:27:35Z
---

## Description
`crates/bitacora-index/src/pipeline/mod.rs`: a `BinaryHeap`-backed priority queue: P0 today's journal + configured home page, P1 pages requested by the UI (`Indexer::request(page_name)` bumps priority), P2 `journals/` newest first, P3 `pages/`. A rayon pool (default `num_cpus - 1`) runs pure `parse()` and forwards `ParsedFile`s to the writer channel with bounded backpressure (e.g. 64 in flight). Expose progress (`files_total`, `files_done`) via `IndexEvent::Progress` for the UI.

## Acceptance Criteria
- Test: with 1,000 queued pages, a `request("Projects")` causes it to be committed before the remaining P3 pages.
- Test: today's journal commits first on a cold build.
- Progress events monotonic and final event equals total.

## Notes
BIT-SP-0003.R6, BIT-SP-0003.R7. [[sqlite-index-schema]] §4.1 step 5.
