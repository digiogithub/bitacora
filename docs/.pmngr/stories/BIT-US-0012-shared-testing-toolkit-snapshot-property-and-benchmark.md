---
id: BIT-US-0012
type: story
title: "Shared testing toolkit: snapshot, property and benchmark harnesses"
status: in_review
priority: high
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, testing]
estimate: 3
created: 2026-10-06T14:26:09Z
updated: 2026-10-06T16:55:34Z
started: 2026-10-06T16:55:34Z
---

## Description
As a developer, I want `insta`, `proptest` and `criterion` wired into the workspace with one working example each, and a common tracing setup for tests, so that the parser round-trip harness (M0) and later index/sync tests follow one convention.

## Acceptance Criteria
- `insta` snapshot test, `proptest` property test and `criterion` bench exist in `bitacora-markdown` as templates (testing a placeholder function) and run in CI (`cargo test`; benches compiled with `cargo bench --no-run`).
- `cargo insta review` workflow and `INSTA_UPDATE=no` in CI are configured so CI never writes snapshots.
- Proptest regressions files are committed and the failure-persistence path is documented in the crate README comment.
- A coverage report (`cargo llvm-cov`) can be produced locally and in an optional CI job.

## Notes
- [[crate-stack]] §4.1 Testing row (`insta 1.49`, `proptest 1.11`, `criterion 0.8.2`).
- AGENTS.md §6 testing expectations. Parser round-trip harness itself belongs to BIT-EP-0003.
