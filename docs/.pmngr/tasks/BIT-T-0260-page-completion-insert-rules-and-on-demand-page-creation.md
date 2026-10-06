---
id: BIT-T-0260
type: task
title: Page completion insert rules and on-demand page creation
status: done
priority: high
parent: BIT-US-0038
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, bitacora-core, autocomplete]
estimate: 2
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
On confirm, replace `[start-2..caret+2]` (for `[[…]]`) with `[[Title]]`, or the hashtag range with `#tag` / `#[[multi word]]` (when the name contains whitespace or special chars per Logseq `page.cljs:792-812`). "New page: <query>" entry submits `CreatePage` (virtual page, no file until content) via the queue. Original title casing of existing pages is used.

## Acceptance Criteria
- Unit tests for hashtag quoting rules.
- New page appears in index/search immediately; no file is created until it has content.

## Notes
Story BIT-US-0038. Implements BIT-SP-0004.R15. Page creation rules on disk belong to BIT-EP-0009.
