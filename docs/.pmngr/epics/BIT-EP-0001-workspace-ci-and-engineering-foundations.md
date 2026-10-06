---
id: BIT-EP-0001
type: epic
title: Workspace, CI and engineering foundations
status: backlog
priority: critical
milestone: BIT-M-0001
author: mcp
labels: [infra]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
Create the Cargo workspace (resolver 3, edition 2024) with the crate layout from [[architecture]] §4 and [[crate-stack]] §5, pinned `[workspace.dependencies]`, toolchain file, profiles, and a GitHub Actions matrix (Linux/macOS/Windows) with fmt, clippy, tests, `cargo deny` (license guard against GPL Zed crates), typos and machete. Add Logseq fixture graphs under `fixtures/graphs/`.

## Acceptance Criteria
- `cargo build --workspace` and `cargo test --workspace` pass on the 3 OSes in CI.
- `cargo deny check` fails the build on GPL dependencies.
- Fixture graphs (Logseq docs graph + edge-case graph) committed with provenance notes.

## Notes
ADR-001, ADR-012, ADR-014.
