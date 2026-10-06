---
id: BIT-T-0335
type: task
title: Externalise UI strings and ship English and Spanish locales
status: todo
priority: medium
parent: BIT-US-0108
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, i18n]
estimate: 2
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T22:24:42Z
---

## Description
Move all user-visible strings in `bitacora-app` to `rust-i18n` locale files `crates/bitacora-app/locales/en.yml` and `es.yml`; locale from OS (`sys-locale`) with `appearance.locale` override applied live; dates in UI formatted per locale (journal titles keep `config.edn` format). Add a CI check (`cargo xtask i18n-check`) that fails on missing keys or hard-coded strings in view code (simple grep allowlist).

## Acceptance Criteria
- 100% key coverage for `en` and `es`; switching locale live re-renders views.

## Notes
[[gpui-and-gpui-kit]] §2.1 (`rust-i18n`). Builds on BIT-US-0025.
