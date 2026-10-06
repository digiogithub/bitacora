---
id: BIT-T-0259
type: task
title: Autocomplete popover component anchored to caret with keyboard navigation
status: backlog
priority: high
parent: BIT-US-0038
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, autocomplete, ui]
estimate: 3
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T14:32:23Z
---

## Description
`crates/bitacora-app/src/editor/autocomplete/popover.rs`: GPUI Kit popover positioned at `BlockEditor` caret bounds, list of results with highlight, `Autocomplete` key context (`Enter` confirm, `Shift+Enter` shift-confirm, `Up/Down`, `Ctrl+P/N`, `Esc` close only). Queries run async against the index (`bitacora-index` page fuzzy search excluding current page; block FTS limit 20 excluding current block and ancestors) with debounced requests and stale-result dropping.

## Acceptance Criteria
- `#[gpui::test]`: Enter in popup confirms and does not split; Esc keeps edit mode.
- Results update within 50 ms on a 10k-page fixture index.

## Notes
Story BIT-US-0038. Implements BIT-SP-0004.R15, BIT-SP-0004.R16. [[sqlite-index-schema]] search API.
