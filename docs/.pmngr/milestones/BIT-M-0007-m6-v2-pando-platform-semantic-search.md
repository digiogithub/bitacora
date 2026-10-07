---
id: BIT-M-0007
type: milestone
title: M6 — v2 Pando platform & semantic search
status: backlog
author: mcp
labels: [v2, pando]
created: 2026-10-07T09:07:50Z
updated: 2026-10-07T09:07:50Z
due: 2027-02-26
---

## Description
Bitacora talks to Pando: a generic Rust SDK (`pando-rs`, Pando repo), the Pando server gaps closed, the `bitacora-pando` adapter crate, a Pando settings panel with per-graph consent, and semantic indexing + hybrid search over graph blocks.

## Acceptance Criteria
- `pando-rs` covers the REST KB API and the AG-UI client and is consumed by Bitacora (git rev pin or crates.io).
- Pando settings panel: connection, token (keychain), feature toggles, per-graph consent; nothing leaves the machine without consent.
- Semantic index stays in sync incrementally; search palette offers hybrid results; Pando offline degrades to lexical search.

## Notes
Plan: [[bitacora-v2-plan]] (decisions D1-D4).
