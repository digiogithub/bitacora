---
id: BIT-US-0075
type: story
title: Page view with lazy virtualized outline and navigation history
status: backlog
priority: high
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T14:29:26Z
---

## Description
As a reader, I want to open any page and scroll through long outlines smoothly, expand/collapse blocks visually and go back/forward between pages, so that browsing a large graph feels instant.

## Acceptance Criteria
- Page header: title (original name), page properties, alias redirect for empty alias pages, namespace breadcrumb (`a / b / c`) and child namespace list.
- Outline rendered with a virtualized GPUI `list`, loading 50 blocks then 25 per step from `IndexReader::outline`; collapsed subtrees honour `collapsed:: true` and can be toggled in view-only state (not persisted in this epic).
- Placeholder pages (no file) show "No content yet" plus their references.
- Back/forward history (Mod+[ / Mod+]) across pages and scroll positions.
- Page refreshes on `IndexEvent::FileReplaced` for its file without losing scroll position.
- A 5,000-block page scrolls at 60 fps on the reference machine.

## Notes
Implements: BIT-SP-0003.R4, BIT-SP-0003.R7 (consumer). [[04-editor-outliner-operations]] §8 (lazy rendering), [[block-editor]] §7.1. Persisted collapse and zoom are in BIT-EP-0007.
