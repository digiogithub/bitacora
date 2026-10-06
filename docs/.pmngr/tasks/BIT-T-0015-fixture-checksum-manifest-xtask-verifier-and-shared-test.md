---
id: BIT-T-0015
type: task
title: Fixture checksum manifest, xtask verifier and shared test helper
status: done
priority: high
parent: BIT-US-0011
milestone: BIT-M-0001
author: mcp
labels: [infra, fixtures, xtask, testing]
estimate: 2
created: 2026-10-06T14:26:00Z
updated: 2026-10-06T16:55:26Z
started: 2026-10-06T16:46:30Z
closed: 2026-10-06T16:55:26Z
---

## Description
- `cargo xtask fixtures update` writes `fixtures/graphs/MANIFEST.sha256` (one line per file, sorted, relative paths with `/`); `cargo xtask fixtures verify` re-hashes and fails on any mismatch, missing or extra file. Run `verify` in the CI `checks` job and in the `test-app` Windows leg (proves `.gitattributes` works).
- Add a small dev-only helper crate `crates/bitacora-testkit` (`publish = false`, no deps on other bitacora crates) exposing `fixtures_root() -> PathBuf`, `graph(name) -> PathBuf`, and `markdown_files(name) -> impl Iterator<Item = PathBuf>` (sorted, using `walkdir`), for use as a `[dev-dependencies]` entry.

## Acceptance Criteria
- Modifying one byte of any fixture makes `cargo xtask fixtures verify` fail with the file name.
- `bitacora-testkit` is only ever a dev-dependency (enforced in `cargo xtask check-deps`).
- `cargo test -p bitacora-testkit` lists both fixture graphs.

## Notes
- AGENTS.md §6 (round-trip tests over `fixtures/graphs/**`). [[crate-stack]] §5.1 (`fixtures/graphs/`).
