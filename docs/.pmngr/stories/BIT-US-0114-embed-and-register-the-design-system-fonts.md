---
id: BIT-US-0114
type: story
title: Embed and register the design system fonts
status: backlog
priority: high
parent: BIT-EP-0015
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, bitacora-app, fonts]
estimate: 3
created: 2026-10-07T09:11:26Z
updated: 2026-10-07T09:11:26Z
---

## Description
As a user, I want the app to look the same on every machine, so that the UI does not depend on installed fonts.

## Acceptance Criteria
- Static TTFs of Atkinson Hyperlegible Next, Atkinson Hyperlegible Mono and Literata embedded (`include_bytes!`) and registered via `text_system().add_fonts` before the first window.
- OFL licence texts shipped (repo + bundles); `cargo deny`/licence notes updated.
- Test: the text system resolves all three families (handle "Literata 36pt" naming); existing font-family/font-size settings still override.

## Notes
Implements BIT-SP-0008.R3. Source `/www/Bitacora/bitacora-design-system/fonts/`.
