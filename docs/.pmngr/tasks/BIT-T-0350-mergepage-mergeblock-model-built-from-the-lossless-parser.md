---
id: BIT-T-0350
type: task
title: MergePage/MergeBlock model built from the lossless parser with normalization
status: backlog
priority: critical
parent: BIT-US-0049
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T14:34:49Z
---

## Description
`crates/bitacora-sync/src/merge/model.rs`: build `MergePage { pre_block: Option<PropBlock>, blocks: Vec<MergeBlock>, style: FileStyle{indent, eol, bom} }` and `MergeBlock { key: BlockKey{Id(Uuid)|Synthetic(u64)}, marker, content (first line + continuation, no props/meta), props: IndexMap<String,String> (content-class keys), meta: Meta{collapsed, id, logbook: BTreeSet<Clock>, card: Option<CardGroup>, lww: IndexMap<String,String>, schedule, deadline}, children, raw: Span }` from `bitacora_markdown` output. Property classification table from [[02-markdown-block-syntax]] §5.4 in `merge/classify.rs` (Identity / Content / Metadata). `normalized_hash()` ignoring trailing whitespace, CRLF, indent unit, property order.

## Acceptance Criteria
- Tests: classification of every §5.4 key; raw spans re-concatenate to original bytes for fixture pages.

## Notes
Story BIT-US-0049. Implements BIT-SP-0006.R9, BIT-SP-0006.R11. ADR-003, ADR-009.
