---
id: BIT-T-0029
type: task
title: Scheduled advisories check and Dependabot configuration
status: backlog
priority: medium
parent: BIT-US-0013
milestone: BIT-M-0001
author: mcp
labels: [ci, dependencies, security]
estimate: 1
created: 2026-10-06T14:26:40Z
updated: 2026-10-06T14:26:40Z
---

## Description
- `.github/workflows/audit.yml`: daily `cargo deny check advisories` (cargo-deny-action) on ubuntu.
- `.github/dependabot.yml`: `github-actions` weekly; `cargo` weekly, grouped minor/patch updates, `ignore: gpui-kit, gpui-pre*` (upgraded deliberately via the canary), and `rmcp` limited to patch updates (`~3.5` per [[mcp-server]]).

## Acceptance Criteria
- Dependabot config validates (GitHub shows no config errors).
- Audit workflow runs on schedule and on manual dispatch.

## Notes
- [[crate-stack]] Risk R8; [[mcp-server]] §1 (rmcp minor pin); ADR-001, ADR-010.
