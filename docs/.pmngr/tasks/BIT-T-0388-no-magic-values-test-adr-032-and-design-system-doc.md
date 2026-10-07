---
id: BIT-T-0388
type: task
title: No-magic-values test, ADR-032 and design-system doc
status: backlog
priority: medium
parent: BIT-US-0118
milestone: BIT-M-0006
author: mcp
labels: [v2, tests, docs]
estimate: 2
created: 2026-10-07T09:12:06Z
updated: 2026-10-07T09:12:06Z
---

## Description
Add the grep-style test over `crates/bitacora-app/src/views/**` with an allowlist; add ADR-032 (tokens pipeline) to `docs/architecture.md`; write `docs/design/design-system.md` (English) summarising tokens, fonts, components, amber/accent rules and the regeneration workflow.

## Acceptance Criteria
- Test fails on an injected hex literal; doc linked from architecture overview.
