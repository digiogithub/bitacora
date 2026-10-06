---
id: BIT-US-0001
type: story
title: Cargo workspace skeleton with all crates and pinned dependencies
status: done
priority: critical
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, workspace]
estimate: 5
created: 2026-10-06T14:23:49Z
updated: 2026-10-06T16:44:55Z
started: 2026-10-06T16:38:36Z
closed: 2026-10-06T16:44:55Z
---

## Description
As a developer, I want a compiling Cargo workspace with every Bitacora crate stubbed and all third-party versions pinned in one place, so that every later story starts from the agreed layout and dependency direction instead of inventing it.

This is the very first step of M0. The layout, profiles and pins come from [[crate-stack]] §5.1 and the Recommendations list; the crate responsibilities from [[architecture]] §4.

## Acceptance Criteria
- Root `Cargo.toml` declares `[workspace]` with `resolver = "3"`, `edition = "2024"` in `[workspace.package]`, license `MIT`, and members `crates/*` + `xtask`.
- The 10 crates exist (`bitacora-markdown`, `-config`, `-core`, `-watch`, `-index`, `-merge`, `-sync`, `-mcp`, `-app`, `-cli`) and `cargo build --workspace --locked` succeeds on a clean checkout.
- Dependency direction `markdown` <- `merge` <- `core` <- {`index`, `sync`, `mcp`} <- {`app`, `cli`} holds (`sync` also depends on `merge` directly), `bitacora-merge` depends only on `bitacora-markdown` (ADR-016), and only `bitacora-app` depends on `gpui-kit` (checked automatically).
- `[workspace.dependencies]` contains the pinned versions from [[crate-stack]] Recommendations, with `gpui-kit = "=0.7.1"` as an exact pin.
- `rust-toolchain.toml` and the dev/release profiles from [[crate-stack]] §5.1 are committed; `Cargo.lock` is committed.

## Notes
- [[crate-stack]] §5.1, [[architecture]] §4, [[gpui-and-gpui-kit]] §1.6.
- ADR-001 (gpui-kit exact pin), ADR-012 (core is synchronous, no tokio), ADR-014 (MIT), ADR-016 (`bitacora-merge` crate).
- Git backend crates follow ADR-007 (git CLI + `gix`) as amended by ADR-020 (git not bundled; `gix` must also cover network ops as fallback), not the older git2-first suggestion.
