---
id: BIT-EP-0018
type: epic
title: "pando-rs: Rust SDK for Pando (Pando repo)"
status: backlog
priority: high
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, pando-repo, sdk]
created: 2026-10-07T09:10:26Z
updated: 2026-10-07T09:10:26Z
---

## Description
Create a generic async Rust client `pando-rs` in the Pando repository (`/www/MCP/Pando/pando/sdk/rust/`), next to the TypeScript, Python, .NET and Java SDKs: REST KB API (documents upsert/delete/batch/list, search, reindex), AG-UI client (`RunAgentInput`, typed event enum including Pando extensions `StateDoc` and `pando_permission_request`, SSE parser, `/info`, threads API, cancel, frontend-tool interrupt/resume, HITL helper), auth (`X-Pando-Token`, bearer), conformance tests shared with the Python/TS SDKs, and publication.

## Acceptance Criteria
- Replays the TS/Python AG-UI conformance fixtures; integration test against a real `pando agui-serve` and REST server in Pando CI.
- MIT licensed, `cargo deny` clean, consumable by Bitacora as a pinned git dependency and later from crates.io.

## Notes
- Plan [[bitacora-v2-plan]] decision D1 (ADR-027 in Bitacora). Work happens in the Pando repo; tracked here because Bitacora depends on it.
- There is no Go SDK and no Rust SDK in `sdk/` today (verified 2026-10-07).
- Evaluate `ag-ui-core` (crates.io 0.1, single maintainer) as a type source only.
