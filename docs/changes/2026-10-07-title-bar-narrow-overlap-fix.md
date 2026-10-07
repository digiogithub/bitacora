---
created_at: 2026-10-07T15:58:48.041448233Z
updated_at: 2026-10-07T16:01:23.609589739Z
tags:
    - change
    - fix
    - app
---
# Fix: title-bar search field overlapped other controls in narrow windows

Continues [[bitacora-v2-plan]]; story BIT-US-0128, task BIT-T-0492.

- `views/title_bar.rs` `AppTitleBar`: left/centre/right slots are separate clipping containers; centre slot has a minimum (`center_min_width`), left slot yields first. Debug selectors `title-left`, `title-center`, `title-right`, `window-controls`.
- `views/workspace/top_bar.rs` `Workspace::title_bar(breakpoint, cx)`: Medium shrinks the search (150–300px, label truncates); Narrow collapses it to an icon button opening the palette and drops theme/PDF buttons; tab strip clips.
- `views/workspace.rs`: render passes the `Breakpoint`.

Why: a fixed-width search in a flex centre slot spilled over tabs and buttons when the slot shrank.

Verification: `title_bar_slots_never_overlap_at_any_width` (1400/1000/760/640/480); `cargo test -p bitacora-app` 593 passed; clippy clean. No visual check (headless). Known: many tabs clip the "+" button instead of scrolling.
