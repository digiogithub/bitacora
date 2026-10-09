---
created_at: 2026-10-09T07:20:51.49694938Z
updated_at: 2026-10-09T07:20:51.49694938Z
tags:
    - change
    - app
    - title-bar
---
# BIT-US-0182 merged: title-bar tab overflow dropdown

Merged into main as 16880ce (feature commit f820fb3). Details in [[BIT-US-0182-tab-overflow]]; plan [[bit-m-0011-owner-improvements-plan]].

- Files: `crates/bitacora-app/src/views/workspace/top_bar.rs` (`tab_width_estimate`, `tab_budget`, `tabs_fit`, overflow trigger + `PopoverShell` dropdown), `views/workspace.rs` (`tab_menu_open`, `tabs_collapsed`), `views/kit/tab.rs` (`Tab::suffix`, `Tab::trailing`).
- Why: owner request; many tabs previously clipped the `+` button.
- Verified: bitacora-app 647 tests passed, clippy -D warnings clean. Not visually checked; story in_review.