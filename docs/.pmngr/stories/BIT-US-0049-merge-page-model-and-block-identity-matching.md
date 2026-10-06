---
id: BIT-US-0049
type: story
title: Merge page model and block identity matching
status: done
priority: critical
parent: BIT-EP-0012
milestone: BIT-M-0003
author: mcp
labels: [merge, sync]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T17:17:55Z
started: 2026-10-06T17:08:27Z
closed: 2026-10-06T17:17:55Z
---

## Description
As the merge engine, I want to parse base/ours/theirs pages into block trees and pair up corresponding blocks even when they lack `id::`, so that changes are merged per block rather than per line.

## Acceptance Criteria
- `MergePage { pre_block, blocks }` / `MergeBlock { key, content, props, meta, children, raw }` built from `bitacora-markdown` output with raw spans retained, in `crates/bitacora-merge`.
- Comparison normalization: trailing whitespace, CRLF, indent unit, properties as set, LOGBOOK as set.
- Matching: `id::` → exact content under same parent → LCS of children by content hash → fuzzy (≥ 0.6, best-first, one-to-one) → distinct; duplicate `id::` gets a fresh uuid for the second occurrence.
- Unit tests with base/ours/theirs Markdown fixtures, including short-block ("- TODO") mis-pairing cases.

## Notes
Implements: BIT-SP-0006.R9. See [[git-sync-merge]] §4.1–4.2, [[02-markdown-block-syntax]] §4, [[04-editor-outliner-operations]]. ADR-003, ADR-006, ADR-008.
ADR-016: implemented in the `bitacora-merge` crate, shared by `bitacora-core` (external edits, BIT-US-0069, M2) and `bitacora-sync`; property classification comes from `bitacora-markdown` (BIT-US-0094).
Needed by BIT-US-0069 (ADR-016).
