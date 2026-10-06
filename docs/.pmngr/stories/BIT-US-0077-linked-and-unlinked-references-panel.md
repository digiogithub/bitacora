---
id: BIT-US-0077
type: story
title: Linked and unlinked references panel
status: done
priority: high
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, references, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T19:08:16Z
closed: 2026-10-06T19:08:16Z
---

## Description
As a knowledge worker, I want every page to show which blocks link to it (with their context) and which blocks mention it without linking, so that I can discover connections in my notes.

## Acceptance Criteria
- "Linked references (N)" section below the outline, grouped by page with breadcrumbs, children rendered, collapsed beyond `:ref/default-open-blocks-level`.
- Include/exclude filter popover per referencing page, persisted as the `filters::` page property only once editing exists (read-only: in-memory filters).
- "Unlinked references" section collapsed by default; computed on expand.
- Block ref count bubble on blocks that are referenced; clicking lists referrers inline.
- Linked references of every fixture page match the Logseq expectation fixtures.

## Notes
Implements: BIT-SP-0003.R9, BIT-SP-0003.R17. [[04-editor-outliner-operations]] §9; [[gpui-and-gpui-kit]] §2.2 (Collapsible, List, Popover).
