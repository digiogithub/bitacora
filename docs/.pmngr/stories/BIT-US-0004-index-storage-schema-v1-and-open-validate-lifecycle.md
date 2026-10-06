---
id: BIT-US-0004
type: story
title: Index storage, schema v1 and open/validate lifecycle
status: backlog
priority: critical
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, bitacora-index]
estimate: 5
created: 2026-10-06T14:25:23Z
updated: 2026-10-06T14:25:23Z
---

## Description
As a developer, I want `bitacora-index` to open a per-graph SQLite database in the platform data dir, create schema v1 and decide whether to keep, reparse or rebuild it, so that every other index feature has a reliable, disposable store.

The DB lives at `<data_dir>/bitacora/graphs/<graph-id>/index.sqlite` (ADR-005), never inside the graph. Version mismatch or corruption means delete and rebuild; there are no migrations in v1.

## Acceptance Criteria
- Opening a graph creates the DB under the platform data dir; nothing is written inside the graph folder.
- All tables, FTS tables, triggers and views from [[sqlite-index-schema]] §3 exist after open.
- `user_version` mismatch or a failed `quick_check` deletes and recreates the DB.
- `parser_version` / `config_hash` mismatch flags "full reparse"; `normalizer_version` mismatch flags "FTS rebuild only".
- Read-only connection pool and one write connection are available through a typed API.

## Notes
Implements: BIT-SP-0003.R1, BIT-SP-0003.R2, BIT-SP-0003.R3. ADR-004, ADR-005. See [[sqlite-index-schema]] §1.1, §3, §4.1 step 1.
