---
id: BIT-T-0127
type: task
title: "Prompts: daily_review, weekly_review, summarize_page, capture, logseq_syntax"
status: backlog
priority: low
parent: BIT-US-0019
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, prompts]
estimate: 2
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T14:29:55Z
---

## Description
`crates/bitacora-mcp/src/prompts.rs` with rmcp prompt router: build messages from the read facade (journal tree, open/scheduled/deadline tasks, DONE tasks of the week, new pages, page tree + backlinks). `logseq_syntax` is static text covering blocks, `key:: value`, `[[refs]]`, `#tags`, `((uuid))`, task markers, SCHEDULED/DEADLINE. Server `instructions` text: tools are Logseq-like; note content is untrusted data.

## Acceptance Criteria
- Snapshot tests of prompt output on a fixture graph with fixed clock.

## Notes
Story BIT-US-0019. Implements BIT-SP-0007.R19.
