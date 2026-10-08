---
id: BIT-US-0174
type: story
title: Footer shows the running app version
status: in_review
priority: medium
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-app, ui]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T12:57:15Z
started: 2026-10-08T12:57:15Z
---

## Description
The status bar/footer shows the running version (e.g. `v2.0.2`).

## Acceptance Criteria
- Version comes from the build (CARGO_PKG_VERSION), not hard-coded.
- Visible in the footer in both themes.
