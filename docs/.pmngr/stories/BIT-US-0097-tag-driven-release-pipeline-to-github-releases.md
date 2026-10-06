---
id: BIT-US-0097
type: story
title: Tag-driven release pipeline to GitHub Releases
status: backlog
priority: high
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, ci]
estimate: 5
created: 2026-10-06T14:31:01Z
updated: 2026-10-06T14:31:01Z
---

## Description
As a maintainer, I want pushing a `vX.Y.Z` tag to build, sign and publish all artifacts with checksums and release notes, so that releases are reproducible, one-command events.

## Acceptance Criteria
- `.github/workflows/release.yml` on `v*` tags runs the bundle matrix, smoke tests, and creates a **draft** GitHub Release with all installers, `bitacora-cli` archives, `SHA256SUMS` and generated release notes; a maintainer publishes it manually.
- The tag version must equal the workspace version (job fails otherwise).
- Pre-release tags (`-beta.N`, `-rc.N`) are marked as pre-releases and map to the beta update channel.
- Build provenance attestations (`actions/attest-build-provenance`) are attached to artifacts.

## Notes
- [[crate-stack]] §5.2 (`bundle` job on tags, upload to GitHub Releases, Velopack pack), §5.1 (`xtask`: release notes).
- AGENTS.md §7 (conventional commits feed release notes).
