---
id: BIT-US-0002
type: story
title: "Code-quality and license guards: fmt, clippy lints, cargo-deny, typos, machete"
status: done
priority: critical
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, quality, licensing]
estimate: 3
created: 2026-10-06T14:24:30Z
updated: 2026-10-06T16:45:00Z
started: 2026-10-06T16:44:55Z
closed: 2026-10-06T16:45:00Z
---

## Description
As a maintainer, I want formatting, lint, license and dependency-hygiene rules configured in the repo, so that contributors and coding agents get the same local feedback that CI enforces and GPL Zed crates can never slip in.

## Acceptance Criteria
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`, `typos` and `cargo machete` all pass on the skeleton.
- `cargo deny check licenses` fails when a GPL-2.0/GPL-3.0/AGPL-3.0 crate is introduced (proven by a test fixture or a documented manual check).
- Workspace lints deny `clippy::unwrap_used` / `clippy::expect_used` outside tests (AGENTS.md rule 9).
- The commands in AGENTS.md §4 work verbatim.

## Notes
- [[crate-stack]] §5.2 (`checks` job), [[gpui-and-gpui-kit]] §1.3 and Risk R5 (license contamination).
- ADR-014. AGENTS.md §3 rules 8–9.
