---
id: BIT-T-0070
type: task
title: Fuzzy page titles with nucleo and Reciprocal Rank Fusion
status: done
priority: high
parent: BIT-US-0009
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, search]
estimate: 3
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:35:32Z
started: 2026-10-06T18:26:43Z
closed: 2026-10-06T18:35:32Z
---

## Description
`crates/bitacora-index/src/search/rank.rs`: keep an in-memory `TitleCache` (page id, original name, aliases) refreshed on `IndexEvent`s; run `nucleo-matcher` subsequence scoring. Merge all ranked lists with RRF `score = Σ 1/(60 + rank)`, dedupe by page/block id, boost pages (x1.5) and recent journal days (small decay boost), exact title pinned first.

## Acceptance Criteria
- Test: query `prj mgmt` finds page `Project Management` via fuzzy pass.
- Test: deterministic ordering for ties (by id).
- Title cache update after a page rename is visible to the next search.

## Notes
BIT-SP-0003.R14. Logseq `master:src/main/frontend/worker/search.cljs:738-766`.
