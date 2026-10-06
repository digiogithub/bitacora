---
created_at: 2026-10-06T19:08:27.144855037Z
updated_at: 2026-10-06T19:08:27.144855037Z
tags:
    - change
    - app
---
# Page view, journals feed, history and references (BIT-US-0075/0076/0077)

Continues [[bitacora-full-development-plan]]; builds on [[bit-us-0073-0074-graph-picker-session-block-rendering]] and [[bit-us-0008-0010-read-api-and-cli-doctor]]. Design: [[block-editor]], [[04-editor-outliner-operations]].

## What changed (crates/bitacora-app, commit 4cc8a4c)
- `data.rs`: GraphHandle (IndexReader + root + ViewSettings), IndexResolver (BlockResolver through the index), open_page/page_chunk/zoom_block, linked/unlinked refs models, referrers, journal days/virtual today (jiff local date), event_touches.
- `nav.rs`: Route, NavHistory (bounded, scroll restore).
- `views/page_view.rs`: flat Item list in one gpui list (header, blocks, refs); 50 then 25 blocks; view-only collapse (render/model.rs visible_rows/toggle_row); refresh on IndexEvent (debounced, keeps scroll and collapse overrides); filters popover; unlinked computed on expand; ref count bubble with inline referrers.
- `views/journals.rs`: newest-first feed, virtual today, 7 days per step, blocks read when drawn, 5 s rollover tick.
- `views/main_view.rs`: routing + back/forward (GoBack/GoForward, secondary-[ / secondary-]); `views/block_view.rs`: shared row drawing; session emits Reader(GraphHandle) and post-reconcile Index(IndexEvent); SharedMainView global replaces SharedPageView.

## Verification
fmt, clippy workspace -D warnings, cargo test -p bitacora-app (138 pass), xtask check-deps, machete, deny, typos clean. Screenshots under Xvfb+lavapipe (journals with virtual today, refs, namespace breadcrumb, collapsed ring, ref bubble).

## Notes
Linked refs load all hits at once (1,500 hits ok, no paging). Filter popover and clicks not exercised visually.
