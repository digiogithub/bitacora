---
id: BIT-US-0051
type: story
title: Structural merge and byte-preserving page serialization
status: backlog
priority: critical
parent: BIT-EP-0012
milestone: BIT-M-0004
author: mcp
labels: [merge, sync, serializer]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T14:28:30Z
---

## Description
As a user who also opens the graph in Logseq, I want merges to handle inserted, deleted, moved and reordered blocks and to rewrite only the blocks that changed, so that files never get reformatted and git diffs stay minimal.

## Acceptance Criteria
- Inserts positioned after left sibling match; both-side inserts kept ours-first with dedupe; one-side delete applied; delete-vs-modify (incl. modified child) → conflict; orphaned new child re-attached to nearest ancestor (info note).
- Sibling order 3-way list merge; competing moves → ours + info note; never a conflict.
- Serializer reuses ours' raw spans for unchanged blocks, keeps ours' indentation style, line endings and BOM; output re-parses to the same tree (asserted).
- `merge_page(b, o, t) -> MergeResult { output, conflicts, notes }` short-circuits when o==t, b==o, b==t; falls back to `diff3_lines` on parse failure.

## Notes
Implements: BIT-SP-0006.R12, BIT-SP-0006.R14. See [[git-sync-merge]] §4.3–4.4, [[block-editor]] (serializer). ADR-003, ADR-008.
