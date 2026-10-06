---
id: BIT-US-0001
type: story
title: Cargo workspace skeleton with all crates and pinned dependencies
status: backlog
priority: critical
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, workspace]
estimate: 5
created: 2026-10-06T14:23:49Z
updated: 2026-10-06T14:23:49Z
---

## Description
As a developer, I want a compiling Cargo workspace with every Bitacora crate stubbed and all third-party versions pinned in one place, so that every later story starts from the agreed layout and dependency direction instead of inventing it.

This is the very first step of M0. The layout, profiles and pins come from [[crate-stack]] §5.1 and the Recommendations list; the crate responsibilities from [[architecture]] §4.

## Acceptance Criteria
- Root `Cargo.toml` declares `[workspace]` with `resolver = "3"`, `edition = "2024"` in `[workspace.package]`, license `MIT`, and members `crates/*` + `xtask`.
- The 9 crates exist (`bitacora-markdown`, `-config`, `-core`, `-watch`, `-index`, `-sync`, `-mcp`, `-app`, `-cli`) and `cargo build --workspace --locked` succeeds on a clean checkout.
- Dependency direction `markdown` <- `core` <- {`index`, `sync`, `mcp`} <- {`app`, `cli`} holds, and only `bitacora-app` depends on `gpui-kit` (checked automatically).
- `[workspace.dependencies]` contains the pinned versions from [[crate-stack]] Recommendations, with `gpui-kit = "=0.7.1"` as an exact pin.
- `rust-toolchain.toml` and the dev/release profiles from [[crate-stack]] §5.1 are committed; `Cargo.lock` is committed.

## Notes
- [[crate-stack]] §5.1, [[architecture]] §4, [[gpui-and-gpui-kit]] §1.6.
- ADR-001 (gpui-kit exact pin), ADR-012 (core is synchronous, no tokio), ADR-014 (MIT).
- Git backend crates follow ADR-007 (git CLI + `gix`), not the older git2-first suggestion.
