---
id: BIT-T-0321
type: task
title: "Crash-safety test: kill the writer process mid-write repeatedly"
status: backlog
priority: high
parent: BIT-US-0064
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test]
estimate: 3
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
`crates/bitacora-core/tests/crash_safety.rs` with a helper binary (`[[test]]` harness or `examples/write_loop.rs`) that loops `atomic_write` of alternating 1 MB contents A/B to a target. The test spawns it, kills it (SIGKILL / `TerminateProcess`) after a random 0–50 ms delay, 200 iterations, and after each kill asserts the target hashes to A or B and that a stale temp file, if present, is cleaned by `cleanup_stale_tmp`. Also a fault-injection variant that aborts between write and rename.

## Acceptance Criteria
- Passes on Linux, macOS and Windows CI.
- Never observes a truncated or mixed target.

## Notes
Story BIT-US-0064. Verifies BIT-SP-0005.R5. Epic acceptance: "Killing the process mid-write never leaves a truncated file".
