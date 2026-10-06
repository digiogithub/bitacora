---
id: BIT-T-0031
type: task
title: Create the bitacora_app::ui facade over GPUI Kit components
status: in_progress
priority: high
parent: BIT-US-0014
milestone: BIT-M-0001
author: mcp
labels: [ui, bitacora-app]
estimate: 2
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T16:46:40Z
started: 2026-10-06T16:46:40Z
---

## Description
Add `crates/bitacora-app/src/ui/mod.rs` re-exporting the GPUI and GPUI Kit types the app uses (`pub use gpui_kit::gpui;` plus selected components: Root, Sidebar, Dock, Button, Icon, Notification, Theme ...) and small wrappers where the GPUI Kit API is likely to churn (e.g. `ui::notify(cx, level, msg)`, `ui::icon(name)`). Views elsewhere in the crate import from `crate::ui` only. Add a module-level doc explaining the rule and the upgrade procedure (bump pin -> fix facade -> fix views).

## Acceptance Criteria
- `grep -rn "gpui_kit::" crates/bitacora-app/src --include=*.rs` only matches files under `src/ui/` (enforced by a small test or the xtask check).
- Existing placeholder views compile through the facade.

## Notes
- [[gpui-and-gpui-kit]] Risks R1/R2 ("thin `bitacora-app::ui` facade", "wrap the components we use in our own `ui::` module"). ADR-001.
