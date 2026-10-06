---
id: BIT-T-0005
type: task
title: Configure rustfmt and workspace clippy/rustc lints
status: done
priority: critical
parent: BIT-US-0002
milestone: BIT-M-0001
author: mcp
labels: [infra, quality]
estimate: 1
created: 2026-10-06T14:24:49Z
updated: 2026-10-06T16:45:00Z
started: 2026-10-06T16:44:55Z
closed: 2026-10-06T16:45:00Z
---

## Description
- Add `rustfmt.toml` (edition 2024 style, `newline_style = "Unix"`; avoid nightly-only options).
- In the root `Cargo.toml` add `[workspace.lints.rust]` (`unsafe_code = "warn"`, `missing_debug_implementations = "warn"` where reasonable) and `[workspace.lints.clippy]` (`unwrap_used = "deny"`, `expect_used = "deny"`, `dbg_macro = "deny"`, `todo = "warn"`), and `lints.workspace = true` in every member crate.
- Add `clippy.toml` with `allow-unwrap-in-tests = true` and `allow-expect-in-tests = true`.

## Acceptance Criteria
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passes on the skeleton.
- An `unwrap()` in non-test code of any crate makes clippy fail; one in `#[cfg(test)]` code does not.

## Notes
- AGENTS.md §3 rule 9, §4 commands. [[crate-stack]] §5.2 (`lint` job).
