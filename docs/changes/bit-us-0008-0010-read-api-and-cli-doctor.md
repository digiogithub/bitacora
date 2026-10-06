---
created_at: 2026-10-06T18:40:06.535138274Z
updated_at: 2026-10-06T18:40:06.535138274Z
tags:
    - change
    - index
    - cli
---
# BIT-US-0008 / BIT-US-0010: IndexReader read API, diagnostics, CLI reindex/doctor

Continues [[bitacora-full-development-plan]]; design in [[sqlite-index-schema]] §5.1; builds on [[changes/bit-us-0006-0007-index-writer-and-reconcile.md]].

## What changed
- `crates/bitacora-index/src/read/{mod,outline,refs,unlinked,misc,diagnostics}.rs`: `IndexReader` (`Index::read_api()`): page lookups, `outline` (pagination + collapsed skip), `subtree`, `ancestors`, `block`, `alias_closure`, `alias_redirect`, `linked_references[_with]` (filters:: include/exclude, top-most fold, breadcrumbs), `unlinked_references` (FTS prefilter + Logseq regex, LOGBOOK stripped), block refs, `tasks`, `agenda`, `namespace_children/tree`, `graph_edges`, `diagnostics`, `diagnostic_counts`.
- `crates/bitacora-index/src/diagnostics.rs`: `inspect_index` / `DoctorReport` (quick_check, foreign_key_check, FTS integrity-check; never repairs).
- `parsed.rs`: `DiagnosticKind` + `DuplicatePage`, `CaseConflict`, `from_name`, serde.
- `crates/bitacora-cli/src/cmd/{mod,reindex,doctor}.rs`: `reindex|doctor --graph [--data-dir] [--json]`; doctor exit 2 when unhealthy/missing.
- Refs note: refs are kind-agnostic by page id, so `[[x]]`/`#x`/`#[[x]]` are all found (tested); no ingest change needed.

## Verification
cargo clippy --workspace --all-targets --locked -D warnings clean; bitacora-index tests (read_outline 6, read_refs 11, read_misc 5, read_diagnostics 4 + existing) and bitacora-cli (5 integration + 1 unit) pass; check-deps, cargo deny, machete OK. CLI run on a copy of fixtures/graphs/logseq-docs: 335 files / 907 pages / 6574 blocks in ~300 ms, doctor healthy, graph hashes unchanged.

## Not done
Oracle comparison with Logseq-exported linked-ref counts (needs Logseq tooling); progress bar (no indicatif dependency added).
