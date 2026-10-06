---
id: BIT-T-0057
type: task
title: Fixture graph and no-trace test for discovery and unowned files
status: backlog
priority: high
parent: BIT-US-0027
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, test, compat]
estimate: 2
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:28:27Z
---

## Description
Create `fixtures/graphs/ignore-rules/` containing: `pages/a.md`, `pages/archived.md`, `pages/.drafts/x.md`, `pages/.b.md`, `archived/x.md`, `test.md`, `journals/2025_11_14.md`, `journals/2024_01_01.md`, `logseq/config.edn` with `:hidden ["/archived" "test.md"]`, `logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Desktop.md`, `logseq/.recycle/pages_old.md`, `logseq/version-files/local/x.md`, `logseq/graphs-txid.edn`, `node_modules/m/readme.md`, `whiteboards/Plan.edn`, `draws/2025-11-14-10-30-00.excalidraw`, `x.tldr`, `logseq/custom.css`, `pages/spec.adoc`, a `.git` file with `gitdir: …`, and a symlink created at test time.

Integration test `crates/bitacora-core/tests/discovery.rs`:
- expected ordered scan list;
- "no traces": copy fixture to temp dir, open + index, assert every file's bytes and mtime unchanged and no new files (except none) — R1/R16.

## Acceptance Criteria
- Test passes on Linux/macOS/Windows CI (symlink case skipped on Windows if no privilege).
- Fixture documented in `fixtures/graphs/README.md`.

## Notes
Refs BIT-SP-0002.R1, BIT-SP-0002.R2, BIT-SP-0002.R16.
