---
id: BIT-T-0051
type: task
title: i18n with rust-i18n and the default keymap file
status: in_progress
priority: medium
parent: BIT-US-0025
milestone: BIT-M-0001
author: mcp
labels: [ui, i18n, bitacora-app]
estimate: 2
created: 2026-10-06T14:28:06Z
updated: 2026-10-06T16:46:40Z
started: 2026-10-06T16:46:40Z
---

## Description
- Add `rust-i18n = "4.2"` to `bitacora-app`, `i18n!("assets/locales", fallback = "en")`, and `assets/locales/en.yml` with all shell strings; route every user-visible string through `t!()`. Set the locale once for both Bitacora and GPUI Kit (they share `rust-i18n`).
- Define `actions!(bitacora, [ToggleLeftSidebar, ToggleRightSidebar, ToggleTheme, Quit])` and load bindings from `assets/keymaps/default.json` (embedded with `rust-embed` or `include_str!`), using `cmd-` on macOS and `ctrl-` elsewhere; leave a hook to merge a user keymap later.

## Acceptance Criteria
- No hard-coded user-visible English strings in `src/views/**` (checked by review + a grep test for string literals in `div().child("...")` patterns is optional).
- Keybindings from the JSON file trigger the actions (`#[gpui::test]` with simulated keystrokes).

## Notes
- [[crate-stack]] §4.1 (`rust-i18n 4.2.4`), §5.1 (`assets/locales`, default `keymap.json`). [[gpui-and-gpui-kit]] §1.1 (Actions, Keybindings). Logseq default bindings arrive with the editor (BIT-EP-0007).
