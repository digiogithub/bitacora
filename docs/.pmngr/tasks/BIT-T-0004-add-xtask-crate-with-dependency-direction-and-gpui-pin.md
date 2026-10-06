---
id: BIT-T-0004
type: task
title: Add xtask crate with dependency-direction and GPUI pin checks
status: done
priority: high
parent: BIT-US-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, workspace, xtask]
estimate: 2
created: 2026-10-06T14:24:22Z
updated: 2026-10-06T16:44:55Z
started: 2026-10-06T16:38:36Z
closed: 2026-10-06T16:44:55Z
---

## Description
Create `xtask/` (binary, `publish = false`) invoked via `cargo xtask <cmd>`. First command: `cargo xtask check-deps`, which uses `cargo metadata` (via the `cargo_metadata` crate) to:
1. Fail if any crate other than `bitacora-app` depends (directly or transitively through a workspace crate) on `gpui-kit`, `gpui-pre` or any crate named `gpui*`.
2. Fail if any workspace crate declares a direct `gpui` dependency, or if `gpui-kit` is not pinned with `=`.
3. Fail on a reverse edge in the allowed graph `markdown` <- `merge` <- `core` <- {`index`, `sync`, `mcp`} <- {`app`, `cli`} (`markdown` <- `core`, `merge` <- `sync`, `config` <- `core`, `watch` <- `core`), and if `bitacora-merge` depends on any workspace crate other than `bitacora-markdown` (ADR-016).
4. Fail if `bitacora-core` has `tokio` in its normal dependency closure.
Later stories add `bundle`, `release-notes` and `fixtures` subcommands to this crate.

## Acceptance Criteria
- `cargo xtask check-deps` exits 0 on the skeleton and prints a short report.
- Adding `gpui-kit` to `bitacora-core` or `bitacora-app` -> `bitacora-core` edge in a scratch branch makes it exit non-zero with a clear message (covered by unit tests on a synthetic metadata graph).

## Notes
- [[crate-stack]] §5.1 (`xtask/`), [[gpui-and-gpui-kit]] §1.6 (GPUI Kit's own exact-pin check), ADR-001, ADR-012, ADR-016, AGENTS.md §2.
