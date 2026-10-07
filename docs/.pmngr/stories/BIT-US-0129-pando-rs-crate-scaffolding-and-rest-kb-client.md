---
id: BIT-US-0129
type: story
title: pando-rs crate scaffolding and REST KB client
status: backlog
priority: high
parent: BIT-EP-0018
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, rust]
estimate: 5
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T09:14:29Z
---

## Description
As a Rust client author, I want a `pando-rs` crate in `/www/MCP/Pando/pando/sdk/rust/` with a typed async client for Pando's REST KB API, so that Bitacora and other Rust apps can index and search without reimplementing HTTP details.

## Acceptance Criteria
- Crate (MIT, tokio + reqwest/rustls, thiserror) with CI job in the Pando repo (fmt, clippy, test, `cargo deny`).
- `KbClient`: upsert/delete document, search (`query, limit, tags, path_prefix, scope`), reindex (409 → `ReindexRunning`), embedding-model test; feature-detected batch/list/delete-by-prefix when the server supports them.
- Errors: NotConfigured/Unauthorized/Unreachable/Timeout/Server; token redacted in `Debug`; tests against a mock HTTP server.

## Notes
Plan [[bitacora-v2-plan]] D1. Pando refs: `internal/api/routes.go:155-169`, `handlers_remembrances_kb.go:36`, `handlers_remembrances_search.go:75`, auth `server.go` ~680-720 (`X-Pando-Token`).
