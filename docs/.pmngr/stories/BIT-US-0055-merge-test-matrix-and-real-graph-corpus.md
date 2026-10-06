---
id: BIT-US-0055
type: story
title: Merge test matrix and real-graph corpus
status: done
priority: high
parent: BIT-EP-0012
milestone: BIT-M-0004
author: mcp
labels: [merge, sync, testing]
estimate: 5
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T19:34:18Z
started: 2026-10-06T19:03:20Z
closed: 2026-10-06T19:34:18Z
---

## Description
As a maintainer, I want an exhaustive merge test matrix and a corpus test over real Logseq graphs, so that merge regressions are caught before they can damage users' notes.

## Acceptance Criteria
- Golden tests `fixtures/merge/<case>/{base,ours,theirs,expected}.md` (+ `conflicts.json`) covering metadata-only, content conflict, add/add journal, delete/modify, rename, reorder, LOGBOOK, card-*, id clash, CRLF/tab style; page-level runner in `crates/bitacora-merge/tests`.
- Symmetry property: swapping ours/theirs yields the same block set (ordering rules aside) and the same conflict set.
- Corpus test: for every fixture graph page, generate random edit pairs and assert no markers, round-trip parse, untouched blocks byte-identical.
- End-to-end test: two clones + bare repo, conflicting edits, resolve via API, both converge (in `bitacora-sync`).

## Notes
Implements: BIT-SP-0006.R8, BIT-SP-0006.R9, BIT-SP-0006.R10, BIT-SP-0006.R11, BIT-SP-0006.R12, BIT-SP-0006.R14. See [[git-sync-merge]] §4, AGENTS.md §6.
ADR-016: the golden merge matrix tests `bitacora-merge` in isolation and is reused by the core external-edit tests (BIT-T-0347).
