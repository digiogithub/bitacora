---
id: BIT-T-0084
type: task
title: Call ensure_today on startup and at midnight rollover in the app
status: backlog
parent: BIT-US-0057
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, journals]
estimate: 1
created: 2026-10-06T14:28:51Z
updated: 2026-10-06T14:28:51Z
---

## Description
In `crates/bitacora-app/src/views/journals.rs`, call `bitacora_core::lifecycle::journal_today::ensure_today` via the command queue on graph open and schedule a GPUI timer for the next local midnight to call it again and refresh the Journals view.

## Acceptance Criteria
- `#[gpui::test]`: with a mocked clock advancing past midnight, the Journals view lists the new day on top without writing a file.

## Notes
BIT-SP-0002.R11. Only `bitacora-app` depends on gpui-kit (ADR-001).
