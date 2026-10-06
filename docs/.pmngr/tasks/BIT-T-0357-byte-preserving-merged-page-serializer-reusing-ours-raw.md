---
id: BIT-T-0357
type: task
title: Byte-preserving merged page serializer reusing ours' raw spans
status: backlog
priority: critical
parent: BIT-US-0051
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, bitacora-markdown, merge, serializer]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T14:34:49Z
---

## Description
`crates/bitacora-sync/src/merge/emit.rs`: emit merged tree: blocks whose merged value equals ours (incl. same depth) copy ours' raw bytes; otherwise use `bitacora_markdown` canonical serializer with ours' `FileStyle` (indent unit, EOL, BOM). Blocks taken verbatim from theirs re-indented to target depth using ours' indent unit. After emit, `assert parse(output)` round-trips and yields the merged tree (debug assertion + error in release → fall back to conflict for that file). Never emits marker lines.

## Acceptance Criteria
- Golden tests: CRLF + tab file with one block changed by theirs → diff limited to that block; BOM preserved.

## Notes
Story BIT-US-0051. Implements BIT-SP-0006.R14. ADR-003. See [[block-editor]] serializer.
