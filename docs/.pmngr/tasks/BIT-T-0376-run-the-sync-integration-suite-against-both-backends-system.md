---
id: BIT-T-0376
type: task
title: Run the sync integration suite against both backends (system git hybrid and gix-only)
status: done
priority: high
parent: BIT-US-0045
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, testing, ci]
estimate: 2
created: 2026-10-06T15:15:10Z
updated: 2026-10-06T17:48:56Z
closed: 2026-10-06T17:48:56Z
---

## Description
Parameterise the sync integration tests (`convergence.rs` of BIT-T-0289, `cli_backend.rs`/backend tests, end-to-end conflict test BIT-T-0372, CLI `sync` test of BIT-T-0295) over a `BackendKind { Hybrid, GixOnly }` fixture (e.g. `rstest` cases or a macro), building the engine with the selected backend. Gix-only runs must not spawn `git`: run them with `PATH` stripped of git and assert via a spawn guard. Add a CI matrix dimension so both variants run on Linux, macOS and Windows; the gix-only job also runs on a runner image without git installed (or with git removed from `PATH`).

## Acceptance Criteria
- Every sync integration scenario passes for both backends on 3 OSes.
- A test fails if the gix-only variant spawns a `git` process.

## Notes
Story BIT-US-0045. Verifies BIT-SP-0006.R4, BIT-SP-0006.R5, BIT-SP-0006.R6. ADR-020.
