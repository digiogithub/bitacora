---
id: BIT-US-0129
type: story
title: pando-rs crate scaffolding and REST KB client
status: done
priority: high
parent: BIT-EP-0018
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, rust]
estimate: 5
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T10:11:31Z
closed: 2026-10-07T10:11:31Z
---

## Description
As a Rust client author, I want a `pando-rs` crate in `/www/MCP/Pando/pando/sdk/rust/` with a typed async client for Pando's existing REST KB API, so that any Rust app can index and search without reimplementing HTTP details. It is a generic SDK, like the TS, Python, .NET and Java ones, with no Bitacora-specific code.

## Acceptance Criteria
- Crate (MIT, tokio + reqwest/rustls, thiserror) with a CI job in the Pando repo: fmt, clippy, test, `cargo deny`.
- `KbClient` covers the current routes: upsert and delete document, search (`query, limit, tags, path_prefix, scope, sort_by_date, exclude_outdated`), reindex (409 maps to `ReindexRunning`), embedding-model list and test.
- Errors: NotConfigured, Unauthorized, Unreachable, Timeout, Server. The token is redacted in `Debug`, unknown response fields are tolerated, and tests run against a mock HTTP server.

## Notes
Plan [[bitacora-v2-plan]] D1. Pando refs: `internal/api/routes.go:155-169`, `handlers_remembrances_kb.go:36`, `handlers_remembrances_search.go:75`, auth `server.go` ~680-720 (`X-Pando-Token`).
