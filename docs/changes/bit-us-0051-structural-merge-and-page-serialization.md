---
created_at: 2026-10-06T17:28:09.435672046Z
updated_at: 2026-10-06T17:28:09.435672046Z
tags:
    - change
    - merge
---
# BIT-US-0051: structural merge and byte-preserving page serialization

Continues [[bitacora-full-development-plan]]; designs [[git-sync-merge]] §4.3-4.4; follows [[changes/bit-us-0094-bit-us-0049-bit-us-0050-merge-model-matching-field-merge.md]] and [[changes/bit-us-0092-bit-us-0093-serializer-and-surgical-edits.md]]. ADR-008/009/016.

## What changed (crate bitacora-merge, commits 8d642b0, f7a306c)
- `structure.rs`: `plan()` -> `Plan { order, conflicts, notes }`. Fate per triple (keep / delete / delete-vs-modify keeps the modified side), 3-way parent choice (ours wins competing moves with a note), re-attachment to nearest surviving ancestor, cycle cut, sibling order via `order_siblings` (start from the side that reordered, weave the other side's inserts after their left sibling, ours first).
- `page.rs`: `merge_page(base, ours, theirs, &MergeEnv) -> MergeResult { output, conflicts: Vec<PageConflict>, notes }` with short-circuits, pre-block merge, field merge, id-rewrite of `((uuid))`, output built from ours' `Document` (Original nodes verbatim; theirs blocks written canonically in ours' style; props-only changes via `set_property`/`remove_property`, else rebuilt), re-parse assertion with fallback to `merge_lines` (diff3 on lines). BOM re-added from ours.
- `conflict.rs`: `PageConflict` (block key + breadcrumb), `Note`/`NoteKind`.
- `model.rs`: duplicate-id fresh uuid now deterministic (FNV-1a over id, position, content).
- `matcher.rs`: `salvage_short_edits` last pass: a short block extended/prefixed on one side under the same parent re-pairs (`- Draft` -> `- Draft v2`, `- A` -> `- A!`), required by spec R12 scenarios.
- Tests: `tests/merge_matrix.rs` (28: golden matrix, R12 scenarios, CRLF/BOM/tabs, fixture identity laws, proptests merge(b,x,x)==x, merge(b,b,x)==x byte-exact, append-both-sides keeps bytes, determinism, no markers).

## Notes
- Metadata-only edits do not keep a deleted block alive (delete wins).
- Ours without indented bullets has default indent unit (tab) from bitacora-markdown detection.
- Id rewrite is exact-case on `((lowercase-uuid))`.
- Not done: R14 "id:: added for newly referenced blocks in the same merge commit" (belongs to core/sync integration).

## Verification
fmt; clippy --workspace --all-targets --locked -D warnings clean; `cargo test -p bitacora-merge`: 64 unit + 1 accuracy + 28 matrix pass; `cargo xtask check-deps` OK.
