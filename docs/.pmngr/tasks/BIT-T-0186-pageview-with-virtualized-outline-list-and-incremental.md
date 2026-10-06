---
id: BIT-T-0186
type: task
title: PageView with virtualized outline list and incremental loading
status: done
priority: high
parent: BIT-US-0075
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:08:08Z
closed: 2026-10-06T19:08:08Z
---

## Description
`crates/bitacora-app/src/views/page_view.rs`: `PageView` entity holding `page_id`, a flattened visible-block vector and a GPUI `list` (`ListState`, variable heights). Load 50 blocks via `IndexReader::outline(page_id, 0, 50, skip_collapsed=true)` on a background task, then 25 more when within 10 rows of the end. View-only collapse toggles recompute visibility locally. On `IndexEvent::FileReplaced` for this page: reload and keep scroll anchor by block uuid.

## Acceptance Criteria
- `#[gpui::test]`: 5,000-block page loads first 50 quickly and appends in 25-steps on scroll.
- Refresh after external edit keeps the top visible block in place.
- Manual check: 60 fps scrolling on the 5,000-block page (recorded in PR).

## Notes
BIT-SP-0003.R4. [[04-editor-outliner-operations]] §8 (50/25 lazy rule).
