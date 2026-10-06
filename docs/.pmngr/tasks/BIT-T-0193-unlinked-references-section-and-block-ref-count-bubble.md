---
id: BIT-T-0193
type: task
title: Unlinked references section and block ref count bubble
status: done
priority: medium
parent: BIT-US-0077
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, references]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:08:16Z
closed: 2026-10-06T19:08:16Z
---

## Description
"Unlinked references" `Collapsible`, collapsed by default, calls `IndexReader::unlinked_references` only on first expand (spinner meanwhile), groups like linked refs and highlights the mention. In `BlockView`, show a count `Badge` when `block_ref_count(uuid) > 0`; clicking expands referrers inline under the block.

## Acceptance Criteria
- `#[gpui::test]`: no unlinked query issued until expanded.
- Count bubble appears on a fixture block referenced twice and lists both referrers.

## Notes
BIT-SP-0003.R17, BIT-SP-0003.R8.
