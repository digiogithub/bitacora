---
id: BIT-T-0011
type: task
title: Tune CI caching and build speed
status: in_review
priority: high
parent: BIT-US-0003
milestone: BIT-M-0001
author: mcp
labels: [infra, ci]
estimate: 2
created: 2026-10-06T14:25:20Z
updated: 2026-10-06T16:55:34Z
started: 2026-10-06T16:55:34Z
---

## Description
- Use `Swatinem/rust-cache` (via setup-rust-toolchain) with per-job `shared-key`s and `save-if: ${{ github.ref == 'refs/heads/main' }}`.
- Set `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_INCREMENTAL=0` in CI.
- Use mold on Linux; evaluate `sccache` (`mozilla-actions/sccache-action`) for `test-app` and keep it only if it measurably helps.
- Add `paths-ignore` for `docs/**` (except `docs/.pmngr` changes do not need CI either) so doc-only PRs skip Rust jobs, while keeping a required-status-friendly no-op job.

## Acceptance Criteria
- Total Actions cache usage after a week stays below 10 GB.
- Warm-cache `test-app` time is at least 40% lower than cold (numbers in PR).
- A docs-only PR completes without building Rust code but still reports required checks as passing.

## Notes
- [[crate-stack]] §5.2 "Common setup" (GPUI Kit's practice).
