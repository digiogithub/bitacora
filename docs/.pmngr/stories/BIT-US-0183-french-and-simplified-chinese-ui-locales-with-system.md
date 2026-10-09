---
id: BIT-US-0183
type: story
title: French and Simplified Chinese UI locales with system language detection
status: done
priority: high
parent: BIT-EP-0027
milestone: BIT-M-0011
author: mcp
labels: [bitacora-app, i18n]
estimate: 5
created: 2026-10-09T07:10:42Z
updated: 2026-10-09T12:12:48Z
started: 2026-10-09T07:22:43Z
closed: 2026-10-09T12:12:48Z
---

## Description
`crates/bitacora-app/src/i18n.rs` ships `en` and `es` and already follows the OS via `sys-locale`. Add French (`fr`, "Français") and Simplified Chinese (`zh`, "中文") for every namespace in `assets/locales/` (`<ns>.fr.yml`, `<ns>.zh.yml` / root `fr.yml`, `zh.yml`), including keys added by the print and tab-overflow stories.

- `match_tag`: map `fr_FR`, `fr-CA`, `zh_CN`, `zh-Hans`, `zh-SG` → `fr`/`zh`; Traditional (`zh_TW`, `zh-Hant`, `zh_HK`) also falls back to `zh` (documented) since only Simplified ships.
- Verify system detection on Linux (`LANG`/`LC_ALL`/`LC_MESSAGES`), and that `sys-locale` covers macOS/Windows; add tests for tag matching.
- Language selector in settings lists the 4 languages in native names; default "System".
- CJK rendering: embedded fonts likely lack CJK glyphs; ensure GPUI falls back to a system CJK font (or configure a fallback family list) so Chinese is not tofu. Check on Linux host; document mac/Win as in_review manual check.
- Dates shown in UI per locale where the app already localises them.
- Keep the locale parity test (`every_key_exists_in_all_locales_with_the_same_placeholders`) passing for all 4 locales; update `typos.toml` exclusions for fr/zh files.
- Translations are our own (no copying from Logseq's AGPL translations, ADR-015).

## Acceptance Criteria
- 100% key parity for en/es/fr/zh; placeholders preserved.
- System locale fr_FR / zh_CN starts the app in French / Chinese; explicit setting overrides live.
- Workspace tests and clippy green.
