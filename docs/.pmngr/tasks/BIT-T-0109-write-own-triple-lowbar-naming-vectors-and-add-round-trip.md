---
id: BIT-T-0109
type: task
title: Write own triple-lowbar naming vectors and add round-trip property tests
status: done
priority: critical
parent: BIT-US-0081
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, test, compat]
estimate: 3
created: 2026-10-06T14:29:47Z
updated: 2026-10-06T16:58:59Z
closed: 2026-10-06T16:58:59Z
---

## Description
- Write our own vector set in `crates/bitacora-core/tests/name_sanity.rs` covering the documented naming cases of [[01-file-graph-layout]] §3.2 and the edge cases Logseq's naming tests exercise (facts such as `aa?#/bbb/ccc` → `aa%3F%23___bbb___ccc` and `a__/bbb/ccc` → `a_%5F___bbb___ccc` are fine), with expected outputs verified black-box against Logseq 0.10.15. Do not copy or translate Logseq's `name_sanity_test.cljs`.
- Add the §3.2 table of [[01-file-graph-layout]] as a data file `fixtures/naming/triple_lowbar.tsv` (title, body) shared with the legacy tests.
- Proptest: random titles from an alphabet weighted to reserved chars, `_`, `/`, `%`, `.`, hex digits, NFD sequences; assert `decode(encode(t)) == nfc(make_valid_namespaces(page_name_sanity(t)))` when t has no empty segments.
- Differential oracle (optional, documented, dev-only, not distributed): `tools/logseq-naming-oracle/` script that runs Logseq's own code from the local `../logseq` checkout as a black box (no ported/translated code in the repo) to produce expected outputs for the TSV; not run in CI.

## Acceptance Criteria
- All our vectors pass; proptest 10k cases pass.
- TSV file used by both encoder and decoder tests.

## Notes
Refs BIT-SP-0002.R6. Required by EP-0004 acceptance criteria. ADR-015 (own vectors; Logseq only as black-box oracle in dev-only tooling).
