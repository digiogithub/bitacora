---
id: BIT-T-0350
type: task
title: MergePage/MergeBlock model built from the lossless parser with normalization
status: in_progress
priority: critical
parent: BIT-US-0049
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T17:08:27Z
started: 2026-10-06T17:08:27Z
---

## Description
`crates/bitacora-merge/src/model.rs`: build `MergePage { pre_block: Option<PropBlock>, blocks: Vec<MergeBlock>, style: FileStyle{indent, eol, bom} }` and `MergeBlock { key: BlockKey{Id(Uuid)|Synthetic(u64)}, marker, content (first line + continuation, no props/meta), props: IndexMap<String,String> (content-class keys), meta: Meta{collapsed, id, logbook: BTreeSet<Clock>, card: Option<CardGroup>, lww: IndexMap<String,String>, schedule, deadline}, children, raw: Span }` from `bitacora_markdown` output. Property classification (Identity / Content / Metadata, table of [[02-markdown-block-syntax]] §5.4) is taken from `bitacora-markdown` (BIT-US-0094, BIT-T-0251), not duplicated here (ADR-016). `normalized_hash()` ignoring trailing whitespace, CRLF, indent unit, property order.

## Acceptance Criteria
- Tests: classification of every §5.4 key; raw spans re-concatenate to original bytes for fixture pages.

## Notes
Story BIT-US-0049. Implements BIT-SP-0006.R9, BIT-SP-0006.R11. ADR-003, ADR-009.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
