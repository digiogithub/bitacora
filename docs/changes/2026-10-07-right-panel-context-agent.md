---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - ui
---
# Right panel with Context and Agent tabs (BIT-US-0125)

Part of [[bitacora-v2-plan]].

## What changed
- New `views/right_panel.rs`: `RightPanel` (360px dock content, `Segmented` Context/Agent switch).
  - Context tab: local graph card (moved out of `RightSidebar`), page properties (`properties_table`), backlinks with per-page counts and snippets (`load_context`, background task, reloads on index events), then the existing opened-blocks stack (`RightSidebar`, unchanged behaviour).
  - Agent tab: empty state, "Pando not configured" state (default until `set_agent_configured(true)`), and the slot API `RightPanel::set_agent_slot(Option<AnyView>)` for the M7 chat view.
- `RightSidebar` lost the local graph; gained `navigate()` / `open_in_stack()` helpers for the panel.
- `PaneHub`/`PlaceholderPanel` host the panel; `Workspace` gains `panel`, `right_panel()`, `open_agent_panel()`; right dock width 280 -> 360 (`sidebar_right` token).
- Top bar sparkle opens the right panel on the Agent tab (was: Settings). Locale keys `right.*` in en/es.

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-app --locked`: 487 passed (new: right_panel tests, `sparkle_opens_the_right_panel_on_the_agent_tab`; local graph test moved to `RightPanel`).
