---
id: BIT-T-0007
type: task
title: Configure typos and cargo-machete
status: done
priority: high
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
- Add `_typos.toml` (or `typos.toml`) excluding `fixtures/graphs/**` (user content in many languages must stay byte-exact and must never be "fixed"), `Cargo.lock`, and `docs/.pmngr/**`; add allowed words for Logseq/Bitacora jargon (`Bitacora`, `logseq`, `lowbar`, `LOGBOOK`, `gpui`, `edn`, ...).
- Configure `cargo-machete` ignores via `[package.metadata.cargo-machete]` only where a dependency is used solely through macros.

## Acceptance Criteria
- `typos` and `cargo machete` both exit 0 on the repository at this point.
- `typos` never reports anything under `fixtures/`.

## Notes
- [[crate-stack]] §5.2 `checks` job. AGENTS.md §3 rule 1 (user files are sacred also applies to fixtures).
