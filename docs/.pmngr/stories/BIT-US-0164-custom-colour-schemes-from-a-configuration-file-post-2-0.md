---
id: BIT-US-0164
type: story
title: Custom colour schemes from a configuration file (post-2.0)
status: backlog
priority: low
parent: BIT-EP-0015
author: mcp
labels: [design-system, theme, post-v2]
estimate: 5
created: 2026-10-07T09:55:25Z
updated: 2026-10-07T09:55:25Z
---

## Description
As a user, I want to switch colour schemes defined in a configuration file, so that I can personalise Bitacora beyond the built-in design system themes.

## Acceptance Criteria
- A documented file format (token-based, the same keys as `design/tokens.json`, dark and light variants) loaded from the user profile config dir.
- A scheme picker in Appearance settings, with live reload when the file changes.
- The contrast check is run on load, and failing pairs are reported, not silently applied.

## Notes
Owner decision 2026-10-07: not for 2.0. The Paper theme is removed in v2. Not assigned to a milestone yet.
