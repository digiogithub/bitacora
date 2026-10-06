---
id: BIT-US-0108
type: story
title: Themes, custom.css subset and translations
status: backlog
priority: medium
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [ui, themes, i18n, bitacora-app]
estimate: 5
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T14:31:46Z
---

## Description
As a user, I want to choose light/dark/system and bundled themes, have common `logseq/custom.css` colour and font tweaks applied, and use the app in my language, so that Bitacora feels like my Logseq setup.

## Acceptance Criteria
- Theme picker over the GPUI Kit theme registry plus Bitacora themes (light, dark, system follow), applied live.
- User themes loaded from `<config_dir>/bitacora/themes/*.json`.
- `custom.css` subset: CSS variables `--ls-primary-background-color`, `--ls-primary-text-color`, `--ls-link-text-color`, `--ls-block-bullet-color`, font-family/size on `body`/`.editor` mapped onto theme tokens; unsupported rules listed in a diagnostics panel.
- UI strings externalised with `rust-i18n`; English and Spanish complete; locale follows OS with override.

## Notes
Builds on the theme/i18n infrastructure of BIT-US-0025. [[gpui-and-gpui-kit]] §1.3, §2.2 (Theme). Logseq `custom.css` cannot be fully mapped (see §2.2 note).
