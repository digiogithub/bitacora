---
id: BIT-US-0003
type: story
title: GitHub Actions CI with 3-OS matrix and GPUI-free fast path
status: done
priority: critical
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, ci]
estimate: 8
created: 2026-10-06T14:24:57Z
updated: 2026-10-07T00:14:43Z
started: 2026-10-06T16:55:34Z
closed: 2026-10-07T00:14:43Z
---

## Description
As a developer, I want every push and PR to be checked on Linux, macOS and Windows, with a fast job that does not compile GPUI, so that regressions in parser/core logic are caught in minutes and platform breakage in the app is caught before merge.

## Acceptance Criteria
- Workflow `.github/workflows/ci.yml` runs on `push` to `main` and on PRs, with jobs `checks`, `lint`, `test-core` and `test-app` as in [[crate-stack]] §5.2.
- `test-core` runs on ubuntu without installing GUI system packages and finishes in well under the `test-app` time.
- `test-app` builds and tests `bitacora-app` on `ubuntu-24.04`, `macos-latest` and `windows-latest`.
- Caches stay under the 10 GB GitHub limit (`CARGO_PROFILE_DEV_DEBUG=0`, cache saved only on `main`).
- A failing `cargo deny`, clippy warning or test fails the PR.

## Notes
- [[crate-stack]] §5.2 (CI matrix, Linux packages, cache strategy from GPUI Kit's CI).
- Epic acceptance: build + tests pass on the 3 OSes. ADR-014 (`cargo deny`).
