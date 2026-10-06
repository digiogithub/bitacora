---
id: BIT-T-0072
type: task
title: Setting search.substring to drop or create the trigram block index
status: done
priority: medium
parent: BIT-US-0009
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, search]
estimate: 1
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:35:32Z
started: 2026-10-06T18:26:43Z
closed: 2026-10-06T18:35:32Z
---

## Description
Add `IndexConfig.search_substring: bool` (default true, persisted in `meta`). On open, if false and `blocks_fts_tri` exists: drop table and its trigger statements (regenerate triggers without trigram); if true and missing: create and `'rebuild'`. Engine falls back to `LIKE` when disabled.

## Acceptance Criteria
- Tests matching BIT-SP-0003.R15 scenarios (substring via trigram; disabled → table absent, LIKE still finds `believ`).
- Toggling twice returns to identical search results.

## Notes
BIT-SP-0003.R15.
