---
id: BIT-US-0174
type: story
title: Footer shows the running app version
status: backlog
priority: medium
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-app, ui]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T12:23:05Z
---

## Description
The status bar/footer shows the running version (e.g. `v2.0.2`).

## Acceptance Criteria
- Version comes from the build (CARGO_PKG_VERSION), not hard-coded.
- Visible in the footer in both themes.
