---
id: BIT-T-0423
type: task
title: Create bitacora-pando crate with pinned pando-rs dependency
status: done
priority: high
parent: BIT-US-0135
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 2
created: 2026-10-07T09:16:33Z
updated: 2026-10-07T10:29:46Z
closed: 2026-10-07T10:29:46Z
---

## Description
Add `crates/bitacora-pando` (thiserror errors, tokio runtime handle injected), `pando-rs` as git dependency pinned by rev in `[workspace.dependencies]`, `cargo deny` config updated; facade types for status, KB and agent clients.

## Acceptance Criteria
- `cargo deny check`, clippy and tests green; `cargo tree -p bitacora-core` has no tokio.
