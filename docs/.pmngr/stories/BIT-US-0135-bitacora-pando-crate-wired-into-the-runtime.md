---
id: BIT-US-0135
type: story
title: bitacora-pando crate wired into the runtime
status: in_review
priority: high
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, bitacora-pando, bitacora-runtime]
estimate: 5
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T10:29:46Z
started: 2026-10-07T10:29:46Z
---

## Description
As a developer, I want a `bitacora-pando` crate that owns all Pando communication on top of `pando-rs`, started and stopped by `bitacora-runtime`, so that UI and CLI use one integration point.

## Acceptance Criteria
- New crate in the workspace; depends on `bitacora-config`, `bitacora-core` (types), `bitacora-index`, `pando-rs` (git rev pin); `bitacora-runtime` depends on it; `cargo tree -p bitacora-core` free of tokio/pando.
- Runtime lifecycle: start when enabled, clean shutdown ordering, events to the app over channels (status, sync progress, run events).
- ADR-027 (SDK placement), ADR-028 (crate placement), dependency-direction line in `AGENTS.md`/`architecture.md` updated; `docs/design/pando-integration.md`.

## Notes
Implements BIT-SP-0009.R5. Plan [[bitacora-v2-plan]] D1, D2.
