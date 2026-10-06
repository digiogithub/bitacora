---
id: BIT-T-0003
type: task
title: Scaffold the ten member crates with lib/bin stubs and inter-crate edges
status: done
priority: critical
parent: BIT-US-0001
milestone: BIT-M-0001
author: mcp
labels:
  - infra
  - workspace
  - bitacora-markdown
  - bitacora-config
  - bitacora-core
  - bitacora-watch
  - bitacora-index
  - bitacora-merge
  - bitacora-sync
  - bitacora-mcp
  - bitacora-app
  - bitacora-cli
estimate: 3
created: 2026-10-06T14:24:22Z
updated: 2026-10-06T16:44:55Z
started: 2026-10-06T16:38:36Z
closed: 2026-10-06T16:44:55Z
---

## Description
Create `crates/<name>/Cargo.toml` + `src/lib.rs` (or `src/main.rs`) for:
- libraries: `bitacora-markdown`, `bitacora-config`, `bitacora-core`, `bitacora-watch`, `bitacora-index`, `bitacora-merge`, `bitacora-sync`, `bitacora-mcp`;
- binaries: `bitacora-app` (`[[bin]] name = "bitacora"`), `bitacora-cli` (`[[bin]] name = "bitacora-cli"`, clap skeleton with `serve`, `reindex`, `sync`, `doctor` subcommands that print "not implemented" and exit non-zero).
Each crate inherits `edition`, `license`, `version` and `lints` from the workspace and has a crate-level doc comment summarising its responsibility from [[architecture]] §4.
Wire path dependencies following the allowed direction only: `config` <- `core`; `markdown` <- `merge`; `markdown` <- `core`; `merge` <- `core`, `sync` (`bitacora-merge` depends only on `bitacora-markdown`, ADR-016); `core` <- `watch`, `index`, `sync`, `mcp`; `index`/`sync`/`mcp`/`core` <- `app`, `cli`. Libraries use `thiserror`, binaries `anyhow`. `bitacora-core` must not depend on `tokio`.
Add one trivial unit test per crate so `cargo test --workspace` exercises every crate.

## Acceptance Criteria
- `cargo build --workspace --locked` and `cargo test --workspace --locked` pass.
- `cargo run -p bitacora-cli -- --help` lists the four subcommands.
- `cargo tree -p bitacora-core -e normal | grep tokio` returns nothing.
- `cargo tree -p bitacora-merge -e normal` shows no workspace crate other than `bitacora-markdown`.

## Notes
- [[architecture]] §4, [[crate-stack]] §5.1, AGENTS.md §2.
- ADR-012 (core synchronous), ADR-016 (`bitacora-merge`). Whether `watch`/`config` stay separate crates is an open question in [[crate-stack]]; keep them separate for now.
