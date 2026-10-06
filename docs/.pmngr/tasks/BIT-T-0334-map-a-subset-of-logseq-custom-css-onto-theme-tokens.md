---
id: BIT-T-0334
type: task
title: Map a subset of logseq/custom.css onto theme tokens
status: backlog
priority: low
parent: BIT-US-0108
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, themes]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T14:34:02Z
---

## Description
`crates/bitacora-app/src/theme/custom_css.rs`: parse `logseq/custom.css` with `lightningcss` (or `cssparser`), extract custom properties under `:root`, `html[data-theme=light]`, `html[data-theme=dark]` (`--ls-primary-background-color`, `--ls-secondary-background-color`, `--ls-primary-text-color`, `--ls-link-text-color`, `--ls-block-bullet-color`, `--ls-tag-text-color`, ...) and `font-family`/`font-size` on `body`/`.editor`, and overlay them onto the active theme tokens; list ignored rules in a diagnostics view. Never modify `custom.css`.

## Acceptance Criteria
- Unit tests over 3 sample custom.css files from popular Logseq themes: mapped tokens and ignored-rule counts.
- Setting `appearance.apply_custom_css` (default on) toggles live.

## Notes
[[gpui-and-gpui-kit]] §2.2 (Theme: full custom.css mapping not possible).
