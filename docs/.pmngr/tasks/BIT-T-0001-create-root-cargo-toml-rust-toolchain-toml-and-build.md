---
id: BIT-T-0001
type: task
title: Create root Cargo.toml, rust-toolchain.toml and build profiles
status: done
priority: critical
parent: BIT-US-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, workspace]
estimate: 2
created: 2026-10-06T14:24:22Z
updated: 2026-10-06T16:44:55Z
started: 2026-10-06T16:38:36Z
closed: 2026-10-06T16:44:55Z
---

## Description
Create the workspace root:
- `Cargo.toml` with `[workspace]` (`resolver = "3"`, `members = ["crates/*", "xtask"]`), `[workspace.package]` (`edition = "2024"`, `license = "MIT"`, `repository`, `rust-version`, `version = "0.1.0"`), and an empty-but-present `[workspace.dependencies]` and `[workspace.lints]` section (filled by sibling tasks).
- Profiles from [[crate-stack]] §5.1: `[profile.dev] debug = "line-tables-only"`, `[profile.dev.package."*"] opt-level = 1`, `[profile.release] lto = "thin"`, `codegen-units = 1`, `strip = "debuginfo"`, `panic = "unwind"`.
- `rust-toolchain.toml` pinning the latest stable channel with `rustfmt` and `clippy` components.
- `.cargo/config.toml` with optional `mold` linker on Linux (commented guidance, not mandatory for contributors) and an `xtask` alias (`xtask = "run --package xtask --"`).

## Acceptance Criteria
- `cargo metadata --format-version 1` succeeds at the repo root.
- Profiles match [[crate-stack]] §5.1 exactly.
- `rustup show` in the repo selects the pinned toolchain.

## Notes
- [[crate-stack]] §5.1, ADR-014 (MIT). `.gitignore` already ignores `/target/`.
