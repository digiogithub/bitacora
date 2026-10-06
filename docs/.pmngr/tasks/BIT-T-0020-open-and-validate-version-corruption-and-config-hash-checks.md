---
id: BIT-T-0020
type: task
title: "Open and validate: version, corruption and config-hash checks"
status: backlog
priority: critical
parent: BIT-US-0004
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T14:26:33Z
---

## Description
`crates/bitacora-index/src/open.rs`: `fn open(graph: &GraphRef, cfg: &IndexConfig) -> Result<(Index, OpenOutcome), IndexError>` where `OpenOutcome = Fresh | UpToDate | FullReparse | FtsRebuild | RebuiltAfterCorruption`.
- `PRAGMA user_version != SCHEMA_VERSION` → close, delete `index.sqlite`, `-wal`, `-shm`, recreate.
- `SQLITE_CORRUPT`/`SQLITE_NOTADB` on open or `PRAGMA quick_check` != `ok` → delete and recreate.
- `meta.parser_version` != `PARSER_VERSION` or `meta.config_hash` != `config_hash(cfg)` → `FullReparse`.
- `config_hash` = blake3 over a canonical EDN rendering of the relevant keys only (`:journal/page-title-format`, `:file/name-format`, `:journals-directory`, `:pages-directory`, `:whiteboards-directory`, `:hidden`, `:property/separated-by-commas`, `:property-pages/enabled?`, `:property-pages/excludelist`, `:ignored-page-references-keywords`, `:preferred-format`), read via `bitacora-config`.
- `normalizer_version` mismatch → `FtsRebuild`.

## Acceptance Criteria
- Tests: version bump, garbage bytes in the DB file, relevant vs irrelevant config change (`:ui/show-brackets?` → `UpToDate`).
- `meta` populated with `schema_version`, `parser_version`, `config_hash`, `normalizer_version`, `graph_root`, `created_at`, `sqlite_version`.

## Notes
BIT-SP-0003.R2. [[sqlite-index-schema]] §1 principle 2, §4.1 step 1.
