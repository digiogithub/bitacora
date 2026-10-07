---
id: BIT-US-0133
type: story
title: Prefix-scoped tokens and versioned REST contract
status: cancelled
priority: medium
parent: BIT-EP-0019
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, security, api]
estimate: 3
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T09:53:36Z
closed: 2026-10-07T09:53:36Z
---

## Description
As a privacy-conscious user, I want Bitacora's Pando token limited to its own KB prefix, and clients able to detect API capabilities.

## Acceptance Criteria
- Optional additional tokens scoped to a `path_prefix` (and to KB routes only); requests outside the scope get 403.
- `/info` (and/or `/api/v1/version`) reports API version and capability flags (batch, list, hash_skip, delete_prefix, index_only, scoped_tokens).
- REST contract documented (OpenAPI or Markdown) with compatibility policy.

## Notes
Pando auth: `server.go` ~596 (`generateToken`), ~680-720 (`authMiddleware`).
