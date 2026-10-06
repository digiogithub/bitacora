---
id: BIT-T-0026
type: task
title: Search text normalizer (built-in props stripped, NFKC, case and accent folding)
status: done
priority: high
parent: BIT-US-0005
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, search]
estimate: 2
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T17:32:45Z
closed: 2026-10-06T17:32:45Z
---

## Description
`crates/bitacora-index/src/normalize.rs`: `pub fn normalize(s: &str, opts: &NormalizeOpts) -> String` = drop hidden built-in property lines (`id::`, `collapsed::`, `heading::`, `created-at::`, `updated-at::`, `card-*`, ...) → NFKC (`unicode-normalization`) → lower-case → optional diacritic stripping (NFD + remove `Mn`) → truncate at `search.max_block_len` (default 10,000 chars, on a char boundary) and report truncation. `const NORMALIZER_VERSION`. Used for `blocks.search_text`, `pages.search_title` (original name + aliases) and for user queries.

## Acceptance Criteria
- Tests: `Reunión con Álvaro` → `reunion con alvaro`; full-width `ＡＢＣ` → `abc`; `id::` line removed; 25,000-char block truncated and `too_large` diagnostic emitted.
- Bumping `NORMALIZER_VERSION` triggers `OpenOutcome::FtsRebuild`.

## Notes
BIT-SP-0003.R13. [[sqlite-index-schema]] §6.1.
