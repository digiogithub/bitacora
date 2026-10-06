---
id: BIT-T-0076
type: task
title: "bitacora-cli doctor: index integrity and diagnostics report"
status: backlog
priority: medium
parent: BIT-US-0010
milestone: BIT-M-0002
author: mcp
labels: [bitacora-cli, bitacora-index]
estimate: 2
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T14:28:35Z
---

## Description
`crates/bitacora-cli/src/cmd/doctor.rs` (index section): print index path, schema/parser versions, SQLite version, size; run `PRAGMA quick_check`, `PRAGMA foreign_key_check`, `INSERT INTO blocks_fts(blocks_fts) VALUES('integrity-check')` (and for `blocks_fts_tri`, `pages_fts`); list diagnostics grouped by kind with file:line; suggest `reindex` on any failure. `--json` output.

## Acceptance Criteria
- Integration test: healthy fixture → all checks `ok`; a fixture with duplicate titles lists the `duplicate_page` diagnostic; a corrupted DB file reports failure with exit code 2.

## Notes
BIT-SP-0003.R11, BIT-SP-0003.R2.
