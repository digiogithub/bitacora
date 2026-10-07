---
id: BIT-T-0384
type: task
title: Keep Paper theme, user themes and custom.css precedence
status: backlog
priority: medium
parent: BIT-US-0116
milestone: BIT-M-0006
author: mcp
labels: [v2, theme, compat]
estimate: 2
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
Keep the 1.x "Paper" theme selectable, migrate users who had it explicitly selected, and keep user theme files and `logseq/custom.css` overrides layered over the token defaults.

## Acceptance Criteria
- Existing `custom_css.rs` and theme tests pass; new test: a user with Paper selected in 1.x keeps it after upgrade.
