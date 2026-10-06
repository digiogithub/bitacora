---
created_at: 2026-10-06T17:32:59.353367048Z
updated_at: 2026-10-06T17:32:59.353367048Z
tags:
    - change
    - index
---
# BIT-US-0005: ParsedFile index rows (T-0022..0027)

Part of [[bitacora-full-development-plan]]; design [[sqlite-index-schema]] section 4.3, [[03-parsing-indexing-search]]; builds on [[bit-us-0004-index-storage-schema-v1]] and [[bit-us-0083-0084-0059-inline-scanner-tasks-page-props]]. Commit 2485dd7.

## What changed (crates/bitacora-index)
- `src/parsed.rs`: ParsedFile, ParsedBlock, PageDef, ParsedProperty(+values), PageRefKind (1..8), BlockRefKind (1..3), Diagnostic, `path_refs`/`ancestors`.
- `src/parse.rs`: `parse(&GraphPath, &[u8], &ParseConfig) -> ParsedFile`, PARSER_VERSION=1, `builtin_level`. Pre-order ord/subtree_end/depth/sibling_idx/parent_ord (pre-block = ord 0 and counts as first top-level sibling), refs (marker, priority, link, tag, embed, property value/name, namespace parents), EAV typing, task columns with raw timestamps, id::/collapsed/heading/created-at, diagnostics, byte spans (BOM-adjusted), 1-based lines.
- `src/normalize.rs`: built-in property stripping, NFKC, lowercase, accent folding, 10 000 char truncation, NORMALIZER_VERSION.
- xtask deps table: index may depend on bitacora-config and bitacora-markdown directly.
- Tests: `tests/parse_unit.rs` (16), `tests/parse_golden.rs` + insta snapshots (edge-cases full, legacy-names full, logseq-docs summary) with interval invariants over every file.

## Notes / limits
- org and other files: page only, Unsupported diagnostic; non-UTF-8 markdown: no blocks + ParseError.
- Link vs tag when a page is written both ways collapses to Tag; embed-only pages get kind 8.
- YAML front matter `tags: [a, b]` yields `[a`, `b]` (faithful to current interpret; unverified against Logseq oracle).
- Directive/front-matter properties deep in the page (not in a pre-block) feed aliases/tags but no block rows.

## Verification
fmt, clippy workspace -D warnings clean; `cargo test -p bitacora-index --locked`: 5 unit + 18 lifecycle + 3 golden + 16 parse tests pass; check-deps, cargo deny, machete OK.
