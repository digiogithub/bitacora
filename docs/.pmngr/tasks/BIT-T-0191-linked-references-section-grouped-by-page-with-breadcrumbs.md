---
id: BIT-T-0191
type: task
title: Linked references section grouped by page with breadcrumbs
status: backlog
priority: high
parent: BIT-US-0077
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, references]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T14:30:56Z
---

## Description
`crates/bitacora-app/src/views/references.rs`: `LinkedReferences` view under the page outline fed by `IndexReader::linked_references(page_id, filters)` on a background task; `Collapsible` per referencing page (journals newest first), each hit with breadcrumb (`IndexReader::ancestors`) and its subtree rendered with `BlockView`, collapsed beyond `:ref/default-open-blocks-level`. Refresh on `IndexEvent`s touching the page or its referrers.

## Acceptance Criteria
- `#[gpui::test]`: counts and groups match `IndexReader` output for a fixture page.
- Fixture expectation test: groups and counts for each fixture page match the Logseq-exported JSON.

## Notes
BIT-SP-0003.R9.
