---
id: BIT-T-0218
type: task
title: "Release workflow: build matrix, draft release, checksums and attestations"
status: backlog
priority: high
parent: BIT-US-0097
milestone: BIT-M-0005
author: mcp
labels: [release, ci]
estimate: 3
created: 2026-10-06T14:31:13Z
updated: 2026-10-06T14:31:13Z
---

## Description
Create `.github/workflows/release.yml` (trigger: push tags `v*`; also `workflow_dispatch` with a dry-run input): job `verify` (tag == `cargo metadata` workspace version, CHANGELOG/notes present), `bundle` matrix (macos-latest aarch64 + x86_64 or universal, windows-latest, ubuntu-24.04) calling `cargo xtask bundle --sign`, `bundle-smoke`, then `publish`: download artifacts, compute `SHA256SUMS`, `actions/attest-build-provenance`, and `gh release create --draft --verify-tag` (`--prerelease` for `-beta`/`-rc`). Permissions minimal per job (`contents: write` only on publish, `id-token: write` + `attestations: write` for provenance). Concurrency: one release per tag.

## Acceptance Criteria
- Dry-run on a fork/test tag produces a draft release with all expected files and a valid `SHA256SUMS`.
- Mismatched tag/version fails at `verify` before any build.

## Notes
- [[crate-stack]] §5.2 (`bundle` job).
