---
id: BIT-T-0507
type: task
title: "Keymap panel: group rows by context, record/persist test"
status: done
parent: BIT-US-0170
milestone: BIT-M-0006
author: mcp
created: 2026-10-07T19:25:19Z
updated: 2026-10-07T19:25:19Z
---

Context group headers in views/settings/keymap.rs; test recording_a_keystroke_binds_it_and_a_restart_reloads_the_override (record, modifier-only ignored, live apply, reload). Verified: cargo test -p bitacora-app (613 pass), clippy clean. Existing tests already cover conflict, reset, persistence.
