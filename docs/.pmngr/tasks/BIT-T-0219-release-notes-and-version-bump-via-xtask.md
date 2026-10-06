---
id: BIT-T-0219
type: task
title: Release notes and version bump via xtask
status: done
priority: medium
parent: BIT-US-0097
milestone: BIT-M-0005
author: mcp
labels: [release, xtask]
estimate: 2
created: 2026-10-06T14:31:13Z
updated: 2026-10-06T19:59:49Z
closed: 2026-10-06T19:59:49Z
---

## Description
- `cargo xtask release-notes <from>..<to>`: group conventional commits (`feat`, `fix`, `perf`, `refactor`, breaking `!`) by crate scope, include `Refs: BIT-...` ids, output Markdown (use `git-cliff` as a library/CLI with a committed `cliff.toml`, or a small parser over `git log`).
- `cargo xtask bump <major|minor|patch|X.Y.Z[-pre]>`: update `[workspace.package] version`, `Cargo.lock`, prepend `CHANGELOG.md`, and print the `git tag` command (never pushes).

## Acceptance Criteria
- Unit tests on a synthetic commit list produce the expected grouped Markdown.
- `bump` leaves `cargo build --locked` working.

## Notes
- AGENTS.md §7 (conventional commits, scope = crate, `Refs:` trailer); [[crate-stack]] §5.1 (`xtask`: release notes).
