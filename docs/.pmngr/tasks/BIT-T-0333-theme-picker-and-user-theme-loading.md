---
id: BIT-T-0333
type: task
title: Theme picker and user theme loading
status: done
priority: medium
parent: BIT-US-0108
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, themes, ui]
estimate: 2
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T22:44:03Z
started: 2026-10-06T22:44:00Z
closed: 2026-10-06T22:44:03Z
---

## Description
`crates/bitacora-app/src/theme/mod.rs`: theme picker (`Select` with live preview) over the GPUI Kit theme registry plus Bitacora light/dark themes; "System" follows OS appearance changes; load user themes from `<config_dir>/bitacora/themes/*.json` (GPUI Kit theme JSON schema) with validation errors shown as a notification; hot reload on file change.

## Acceptance Criteria
- `#[gpui::test]`: switching theme updates `cx.theme()`; invalid JSON theme is skipped with a notification.

## Notes
[[gpui-and-gpui-kit]] §1.3, §2.2 (Theme). Builds on BIT-US-0025.
