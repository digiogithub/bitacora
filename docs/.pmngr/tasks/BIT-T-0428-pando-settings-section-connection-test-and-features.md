---
id: BIT-T-0428
type: task
title: "Pando settings section: connection, test and features"
status: backlog
priority: high
parent: BIT-US-0137
milestone: BIT-M-0007
author: mcp
labels: [v2, settings, bitacora-app]
estimate: 3
created: 2026-10-07T09:16:33Z
updated: 2026-10-07T09:16:33Z
---

## Description
`views/settings/pando.rs`: connection form, token entry, Test connection (version, agents, model from `/info`), minimum-version check, feature switches with profile pickers, graphs subsection (consent, exclusions, write grant).

## Acceptance Criteria
- `#[gpui::test]` for validation errors and successful test against a mock server.
