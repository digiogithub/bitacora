---
id: BIT-EP-0020
type: epic
title: "Pando integration core: bitacora-pando crate, settings panel and consent"
status: backlog
priority: high
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, bitacora-pando, settings, security]
created: 2026-10-07T09:10:54Z
updated: 2026-10-07T09:10:54Z
---

## Description
New crate `bitacora-pando` (adapter over `pando-rs`) wired into `bitacora-runtime`; machine-local Pando settings model with keychain tokens; Pando settings panel (button in the top bar/sidebar + settings window section) with connection test, profiles, per-feature switches; per-graph consent and exclusions; auto-provisioned Read-only `pando` MCP token registered in Pando; connection status and graceful degradation; Pando activity log; ADRs and design doc.

## Acceptance Criteria
- BIT-SP-0009.R1-R7 satisfied and verified.
- `cargo tree -p bitacora-core` shows no tokio/pando dependency; `cargo deny check` passes.

## Notes
- Plan [[bitacora-v2-plan]] decisions D1-D3, D5 (ADR-027..029, ADR-031).
- Compare with git-in-track `internal/config/pando.go`, `pandomode.go` (modes auto/managed/external/off, `allowRemote`, env tokens never serialised).
