---
id: BIT-T-0514
type: task
title: Manual visual check of special blocks and close BIT-US-0169
status: backlog
priority: medium
parent: BIT-US-0188
milestone: BIT-M-0006
author: mcp
estimate: 1
created: 2026-10-13T07:57:55Z
updated: 2026-10-13T07:57:55Z
---

## Description
Run `cargo run -p bitacora-app -- --graph fixtures/graphs/edge-cases`, insert every `<` command, check read mode in light and dark (callout icon/title per kind, quote rule, code copy button, center/verse/example). Record screenshots in the change doc, then move BIT-T-0505 and BIT-US-0169 to done.

## Acceptance Criteria
- All 16 `<` commands insert the expected markers and render; findings filed as bugs if any.
