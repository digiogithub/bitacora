---
created_at: 2026-10-06T19:34:18.184808541Z
updated_at: 2026-10-06T19:34:18.184808541Z
tags:
    - change
    - merge
    - testing
---
# BIT-US-0055: merge test matrix, real-graph corpus, bench, e2e (commit 15c5ddb)

Part of [[bitacora-full-development-plan]]; designs: [[git-sync-merge]]. Continues [[changes/bit-us-0051-structural-merge-and-page-serialization.md]] and [[changes/bit-us-0052-0053-file-merge-policies-and-conflict-state.md]].

## What changed
- `fixtures/merge/<case>/{base,ours,theirs,expected}.md` (+ `conflicts.json`): 23 cases (metadata-only, content conflict, add/add journal, delete/modify, delete-unchanged, rename-edit title, reorder, reorder-vs-edit, move, id union, logbook union, CRLF, BOM, tabs vs spaces, card-*, todo markers, both-insert, delete vs metadata/logbook/new id, regression vectors). Runner `crates/bitacora-merge/tests/golden.rs` (`BITACORA_BLESS=1` regenerates; also checks no markers, determinism, symmetric conflict set).
- `crates/bitacora-merge/tests/corpus.rs`: two-device edit simulation over every page of `fixtures/graphs/**` (2322 cases at 6 seeds) with invariants (markers, round-trip, identity laws, token preservation, untouched lines byte-identical, symmetry, idempotence w/o deletes/moves) + proptest fuzz (10k cases clean) + large-page speed guard. Replay: `BITACORA_CORPUS_DEBUG=<page>.md:<seed> cargo test --test corpus replay`.
- `crates/bitacora-merge/benches/merge_large.rs` (criterion): 5k top-level blocks (15k total) disjoint edits 0.28 s (was 1.9 s).
- `crates/bitacora-sync/tests/e2e_conflict.rs`: two clones + bare remote, 1 content conflict + metadata + rename + asset collision, resolve Theirs, convergence; user `git pull` variant. Both backends.

## Bugs fixed (found by the corpus)
1. `matcher.rs` `Pairing::salvage_short` (new pass 5): short parent edited on both sides (`- A` -> `- A x` / property added) was unmatched in base-vs-side matching, so its children were duplicated (`- A x`, `\t- child`, `\t- child`). Mutual-best prefix pairing; children now anchored.
2. `fields.rs` `diff3_touching` used by `page.rs::merge_lines` fallback: one-sided delete next to an inserted property line left an orphan property line on irregular-indent pages. Insert touching a deleted/replaced range now conflicts.
3. `structure.rs::modified`: a delete no longer silently discards new LOGBOOK data or a newly assigned `id::` (raises DeleteVsModify). `collapsed::`/card/volatile metadata still lose to a delete (accepted risk; design 4.3 says metadata is UI state; documented in git-sync-merge.md).
4. Perf: fuzzy matcher precomputes tokens, early-exit banded Levenshtein, prefix/suffix strip (4-7x).

## Known limitations (documented, not fixed)
- Look-alike blocks (`Chapter 4`/`Chapter 5`) can be fuzzy-paired when one disappears; idempotence only asserted without deletes/moves.
- Pages the block merge declines (irregular indentation, runs of empty bullets) use the line fallback, not symmetric by nature.
- Both-side identical inserts at different positions are not deduped.

## Verification
`cargo test -p bitacora-merge -p bitacora-sync --locked`: 261 passed, 0 failed; clippy `-D warnings` clean for both crates; typos clean; `cargo xtask fixtures verify` OK; `cargo deny check` ok; fuzz 10000 cases / 80 seeds clean (release).
