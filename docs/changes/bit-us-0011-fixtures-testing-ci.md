---
created_at: 2026-10-06T16:55:57.613774959Z
updated_at: 2026-10-06T16:55:57.613774959Z
tags:
    - change
    - fixtures
    - testing
    - ci
---
# BIT-US-0011 / 0012 / 0003 / 0013: fixtures, testing toolkit, CI, dependency watch

Continues [[bitacora-full-development-plan]]; see [[crate-stack]] section 5.2, [[02-markdown-block-syntax]], [[01-file-graph-layout]].

## What changed
- `.gitattributes`: `* text=auto eol=lf`, `fixtures/** -text linguist-vendored`, binary types.
- `fixtures/graphs/logseq-docs/`: unmodified MIT snapshot of logseq/docs at b18138a (2025-11-07): pages/, journals/, logseq/config.edn, LICENSE.md, PROVENANCE.md (ADR-021).
- `fixtures/graphs/edge-cases/` and `edge-cases-legacy-names/`: 53 hand-authored files (CRLF, BOM, indentation, pre-blocks, tasks, file-name codecs, bak/.recycle, asset) with PROVENANCE.md mapping file -> quirk -> doc section (ADR-015 clean room).
- `fixtures/graphs/MANIFEST.sha256` (391 files) and `xtask/src/fixtures.rs` (`cargo xtask fixtures update|verify`); `sha2` added to workspace deps.
- `crates/bitacora-testkit`: `fixtures_root`, `graph`, `graph_names`, `markdown_files`, `relative_to_graph`, `init_tracing` (module `tracing_setup`).
- `crates/bitacora-markdown`: dev-deps only; tests/snapshot_template.rs (42 insta snapshots), tests/proptest_template.rs, benches/split.rs, proptest-regressions/.
- `script/install-linux-deps.sh` (--minimal); `.github/workflows/{ci,coverage,audit,gpui-kit-canary}.yml`; `.github/dependabot.yml`; AGENTS.md section 6 pointers.
- xtask check-deps: new test that a normal dependency on bitacora-testkit is rejected.

## Why
Round-trip tests need byte-exact realistic data from day one; CI must be 3-OS with a GPUI-free fast path; GPUI Kit upgrades must be deliberate.

## Verification (local, Linux)
cargo fmt, `cargo clippy --workspace --all-targets --locked -D warnings`, tests for bitacora-markdown/testkit/xtask with INSTA_UPDATE=no, `cargo bench --no-run`, `cargo xtask fixtures verify`, `cargo xtask check-deps`, cargo deny, cargo machete, typos all pass; actionlint clean on all workflows.

## Not verifiable here (stories left in_review)
Remote GitHub Actions runs (green matrix, timings, caches), Windows autocrlf check, install script in a fresh ubuntu:24.04 container, coverage artifact, Dependabot validation, opening edge-case files in Logseq 0.10.15.
