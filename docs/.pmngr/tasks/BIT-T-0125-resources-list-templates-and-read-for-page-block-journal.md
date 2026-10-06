---
id: BIT-T-0125
type: task
title: "Resources: list, templates and read for page/block/journal/config/sync/asset URIs"
status: in_progress
priority: medium
parent: BIT-US-0019
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, resources]
estimate: 3
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
`crates/bitacora-mcp/src/resources.rs`: implement `list_resources` (recent/favourite pages, paged), `list_resource_templates`, `read_resource` for `bitacora://page/{name}`, `bitacora://graph/{graph}/page/{name}`, `bitacora://block/{uuid}`, `bitacora://journal/{yyyy-mm-dd|today}`, `bitacora://graph/{graph}/config` (read-only), `bitacora://sync/status` (JSON), `bitacora://asset/{path}` (blob, size cap 5 MB). Asset path: percent-decode, normalize, reject `..`/absolute, must stay under `<graph>/assets/` after canonicalization.

## Acceptance Criteria
- Tests: page raw text equals file bytes; traversal `bitacora://asset/../logseq/config.edn` → `NOT_FOUND`; oversize asset → error.

## Notes
Story BIT-US-0019. Implements BIT-SP-0007.R18, BIT-SP-0007.R15.
