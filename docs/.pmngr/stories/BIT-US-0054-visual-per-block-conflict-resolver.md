---
id: BIT-US-0054
type: story
title: Visual per-block conflict resolver
status: backlog
priority: high
parent: BIT-EP-0012
milestone: BIT-M-0004
author: mcp
labels: [merge, ui, conflicts]
estimate: 13
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T14:28:30Z
---

## Description
As a user, I want one screen listing every conflicting block side by side with Keep mine / Keep theirs / Keep both / Edit, so that I can resolve a sync's conflicts quickly without ever seeing git markers.

## Acceptance Criteria
- "Conflicts (N)" banner in status bar and on affected page headers; conflicted blocks show a subtle marker and jump to the resolver.
- Cards grouped by page with breadcrumb, ours | theirs rendered side by side with word-level diff vs base, theirs author/time.
- Actions: Keep mine, Keep theirs, Keep both (theirs as next sibling, regenerated `id::` for the copy), Edit (inline editor seeded with both versions, no markers); delete-vs-modify: Keep (modified)/Delete; rename_rename/config/binary variants.
- Bulk "resolve all on this page with mine/theirs"; keyboard shortcuts; editing a conflicted block in the page editor counts as Edit.
- `#[gpui::test]` coverage for resolution state transitions.

## Notes
Implements: BIT-SP-0006.R16. See [[git-sync-merge]] §4.6, [[block-editor]], [[gpui-and-gpui-kit]]. ADR-001, ADR-002, ADR-009.
