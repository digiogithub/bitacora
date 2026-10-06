---
id: BIT-T-0142
type: task
title: Implement legacy encoder/decoder and legacy-dot detector
status: backlog
priority: critical
parent: BIT-US-0085
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:30:11Z
updated: 2026-10-06T14:30:11Z
---

## Description
`crates/bitacora-core/src/naming/legacy.rs`:
- `decode(body)`: replace every `.` with `/`, then whole-string `decodeURIComponent` semantics (strict UTF-8 percent decoding); on error return the dot-replaced raw string (`graph_parser/util.cljs:244-247`).
- `encode(title)` ("dir-ver 0 after May 2022", `util/fs.cljs:168-181`): URL-encode runs of `[\ # | %]` and runs of `[: * ? " < > |]`, `/` → `%2F`, `*` → `%2A`.
- `is_legacy_dot_name(body)` helper for the older `:legacy-dot` scheme (`fs.cljs:143-164`, reserved → `_`, `/` → `.`) used only for diagnostics.
- `NameCodec` trait implemented by both `TripleLowbar` and `Legacy`, selected from `cfg.name_format()`.

## Acceptance Criteria
- Tests: `Version 1.0` → decode `Version 1/0`; `Projects%2FBitacora` → `Projects/Bitacora`; invalid `%E4` whole-string falls back to raw; encode `Projects/Bitacora` → `Projects%2FBitacora`, `a*b` → `a%2Ab`.
- Shared TSV fixture `fixtures/naming/legacy.tsv`.

## Notes
Refs BIT-SP-0002.R7.
