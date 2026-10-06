---
id: BIT-T-0017
type: task
title: Test tracing setup and optional coverage job
status: in_review
priority: medium
parent: BIT-US-0012
milestone: BIT-M-0001
author: mcp
labels: [testing, ci, bitacora-testkit]
estimate: 1
created: 2026-10-06T14:26:22Z
updated: 2026-10-06T16:55:34Z
started: 2026-10-06T16:55:34Z
---

## Description
- Add `bitacora_testkit::init_tracing()` (idempotent, `tracing-subscriber` with `with_test_writer()` and `RUST_LOG`/`EnvFilter` support) for use in tests.
- Add `.github/workflows/coverage.yml` (manual `workflow_dispatch` + weekly schedule) running `cargo llvm-cov --workspace --exclude bitacora-app --lcov` on ubuntu and uploading the lcov file as an artifact (no third-party upload service).

## Acceptance Criteria
- Calling `init_tracing()` from two tests in the same binary does not panic.
- The coverage workflow produces an `lcov.info` artifact when run manually.

## Notes
- [[crate-stack]] §4.1 (`tracing 0.1.44`, `tracing-subscriber 0.3.23`).
