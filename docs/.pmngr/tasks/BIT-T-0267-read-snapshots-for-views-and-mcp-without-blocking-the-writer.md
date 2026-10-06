---
id: BIT-T-0267
type: task
title: Read snapshots for views and MCP without blocking the writer
status: backlog
priority: high
parent: BIT-US-0062
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, core]
estimate: 2
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T14:32:23Z
---

## Description
After each commit publish immutable `PageSnapshot`s (`Arc`) for touched pages via `arc-swap` (or equivalent) so readers (`PageView`, MCP read tools) get consistent state without locks held across the writer. Snapshot contains block tree, texts and a `version` counter.

## Acceptance Criteria
- Concurrent reader test: readers never observe a half-applied transaction.
- Snapshot publish cost O(touched pages).

## Notes
Story BIT-US-0062. Implements BIT-SP-0005.R1.
