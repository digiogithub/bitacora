---
id: BIT-T-0009
type: task
title: "CI workflow: checks and test-core jobs"
status: done
priority: critical
parent: BIT-US-0003
milestone: BIT-M-0001
author: mcp
labels: [infra, ci]
estimate: 2
created: 2026-10-06T14:25:20Z
updated: 2026-10-06T20:23:04Z
started: 2026-10-06T16:55:34Z
closed: 2026-10-06T20:23:04Z
---

## Description
Create `.github/workflows/ci.yml` with:
- `checks` (ubuntu-latest): `cargo fmt --all --check`, `crate-ci/typos` action, `cargo machete` (via `bnjbvr/cargo-machete` or `taiki-e/install-action`), `EmbarkStudios/cargo-deny-action@v2`, and `cargo xtask check-deps`.
- `test-core` (ubuntu-latest): `cargo test --locked -p bitacora-markdown -p bitacora-config -p bitacora-core -p bitacora-watch -p bitacora-index -p bitacora-sync -p bitacora-mcp -p bitacora-cli` (no GUI packages; uses `script/install-linux-deps.sh --minimal`).
Common: `actions-rust-lang/setup-rust-toolchain@v1` (reads `rust-toolchain.toml`), `rui314/setup-mold@v1`, `env: CARGO_TERM_COLOR=always, CARGO_PROFILE_DEV_DEBUG=0, RUST_BACKTRACE=1`, `concurrency` group cancelling superseded PR runs, `permissions: contents: read`.

## Acceptance Criteria
- Both jobs are green on a PR against the skeleton.
- Introducing a formatting error, a typo, an unused dependency or a denied license each fails `checks`.
- `test-core` does not compile `gpui-kit` (verify in the job log or with `cargo tree`).

## Notes
- [[crate-stack]] §5.2. ADR-014. `bitacora-cli` must not depend on `bitacora-app` (headless).
