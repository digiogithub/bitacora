---
id: BIT-T-0121
type: task
title: Implement list_journals and get_today_journal tools
status: in_progress
priority: medium
parent: BIT-US-0017
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, read, journals]
estimate: 2
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
`crates/bitacora-mcp/src/tools/journals.rs`: `list_journals {from?, to?, limit=7}` → journal pages with day and block count; `get_today_journal {create_if_missing=false}` resolving today with the graph's `:journal/page-title-format` / `:journal/file-name-format` via core; returns the tree (empty virtual page if file absent). `create_if_missing=true` requires write scope (returns `READ_ONLY` until write tools land in BIT-M-0003).

## Acceptance Criteria
- Tests with fixed clock: date formats `yyyy_MM_dd` and custom; virtual empty journal does not create a file.

## Notes
Story BIT-US-0017. Implements BIT-SP-0007.R11. See [[01-file-graph-layout]].
