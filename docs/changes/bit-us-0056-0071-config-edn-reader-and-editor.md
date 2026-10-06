---
created_at: 2026-10-06T16:53:03.381355824Z
updated_at: 2026-10-06T16:53:03.381355824Z
tags:
    - change
    - bitacora-config
---
# BIT-US-0056 + BIT-US-0071: config.edn reader, effective config, editor

Part of [[bitacora-full-development-plan]]; behaviour from [[01-file-graph-layout]] section 2 (ADR-013, ADR-015).

## What changed (crate `bitacora-config`, commit 9b39f4f)
- `src/cst.rs`: `Cst`, `Node`, `Kind`: hand-written lossless EDN tree (whitespace, commas, comments, `#_`, tagged literals); `Cst::print()` reproduces source exactly. No new dependency (`edn-rs` is not lossless, so unused).
- `src/edn.rs`: `Edn` value model, `read_str`, `pr_str`; duplicate map keys/set elements and odd maps are `Diagnostic`s with line/column.
- `src/config.rs`: `EffectiveConfig::{from_texts, load}`, `merge_configs` (later wins, maps shallow-merged), `default_config`, `DEFAULT_CONFIG_EDN` (own text, triple-lowbar). Invalid sources are skipped with a diagnostic; graph still loads.
- `src/accessors.rs`: typed accessors (`name_format`, directories, journal formats, `preferred_format`, `hidden`/`is_hidden`, feature flags, property keys, indentation, favorites, default-home, workflow, meta version).
- `src/edit.rs`: `ConfigEditor` assoc/update_in/dissoc/vec_push/vec_remove_str, favorites and default-home helpers; text splices only, CRLF preserved.
- Tests: `tests/load.rs`, `tests/edit_golden.rs`, fixture `tests/data/commented-config.edn`.

## Why
Spec BIT-SP-0002.R3 (load/merge, legacy default) and R4 (surgical edits).

## Verification
`cargo test -p bitacora-config --locked`: 1 + 22 + 17 passed (1 ignored: Logseq template black-box, passed when run with LOGSEQ_CHECKOUT). `cargo clippy --workspace --all-targets --locked -- -D warnings` clean, `cargo deny check` ok, `cargo xtask check-deps` ok.

## Open points
- rewrite-edn oracle goldens not generated (no Babashka); expectations hand-authored.
- No `fixtures/graphs/**` config.edn existed yet; the round-trip test globs them when present.
