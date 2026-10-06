---
id: BIT-T-0075
type: task
title: bitacora-cli reindex command
status: done
priority: medium
parent: BIT-US-0010
milestone: BIT-M-0002
author: mcp
labels: [bitacora-cli, bitacora-index]
estimate: 2
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:39:52Z
closed: 2026-10-06T18:39:52Z
---

## Description
`crates/bitacora-cli/src/cmd/reindex.rs`: `bitacora-cli reindex --graph <path> [--data-dir <dir>] [--json]` deletes the index file (and `-wal`/`-shm`), runs a cold build with a progress bar (`indicatif`), and prints files/pages/blocks counts, diagnostics count and duration. Exit code 0 on success, 1 on error (anyhow).

## Acceptance Criteria
- Integration test (`assert_cmd`) on a fixture graph: output counts match `canonical_dump` counts; graph folder unchanged (hash all files before/after).

## Notes
BIT-SP-0003.R1, BIT-SP-0003.R16.
