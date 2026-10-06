---
id: BIT-US-0013
type: story
title: Dependency upgrade watch for GPUI Kit and security advisories
status: backlog
priority: medium
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, ci, dependencies]
estimate: 3
created: 2026-10-06T14:26:29Z
updated: 2026-10-06T14:26:29Z
---

## Description
As a maintainer, I want scheduled jobs that try the next GPUI Kit release and re-run advisory checks, so that API churn in the `gpui-pre` snapshots (Risk R1/R2) is discovered early and upgrades are deliberate, not accidental.

## Acceptance Criteria
- A weekly workflow builds `bitacora-app` against the newest `gpui-kit` on crates.io (temporarily rewriting the exact pin) and opens/updates a GitHub issue when it fails or when a newer version compiles.
- A daily `cargo deny check advisories` run fails loudly on new RustSec advisories.
- Dependabot (or Renovate) is configured for GitHub Actions and Cargo with `gpui-kit` excluded from automatic PRs.

## Notes
- [[gpui-and-gpui-kit]] Risks R1, R2 ("scheduled CI job that tries the next GPUI Kit release"), [[crate-stack]] Risk R1, R8.
- ADR-001 (exact pin; upgrades are explicit steps).
