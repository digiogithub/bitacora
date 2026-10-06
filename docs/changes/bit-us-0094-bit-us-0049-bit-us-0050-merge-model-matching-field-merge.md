---
created_at: 2026-10-06T17:18:16.950775157Z
updated_at: 2026-10-06T17:18:16.950775157Z
tags:
    - change
    - merge
    - markdown
---
# BIT-US-0094, BIT-US-0049, BIT-US-0050: classification, page model/matching, field-level block merge

Continues [[bitacora-full-development-plan]]; designs: [[git-sync-merge]] §4.1-4.3, [[02-markdown-block-syntax]] §5.4; ADR-008/009/016.

## What changed
- `crates/bitacora-markdown/src/classify.rs` (new, re-exported in `lib.rs`): `class_of` (single class table, §5.4), `classify_lines`, `parse_block_text` / `BlockParts`, `CanonicalView` / `canonical_view`, `classify_diff` (`DiffClass`), `merge_metadata` (LWW per key, id kept, `IdentityConflict`), `union_logbook` (CLOCK union sorted by start, open clock reduced).
- `crates/bitacora-merge/src/`: `model.rs` (MergePage/MergeBlock arena, normalisation, `normalized_hash`, duplicate-id fresh uuid, FileStyle), `matcher.rs` (`match_blocks`: id, LCS, exact, fuzzy >= 0.6 with < 4-token guard, global fuzzy for moves, then ours<->theirs), `lcs.rs`, `fields.rs` (line `diff3`, `merge_content`, `merge_user_props`), `marker.rs` (`merge_title`, `merge_planning`), `meta.rs` (`MergeEnv`, `merge_meta`: collapsed, id union + `IdRewrite`, LOGBOOK, card group, LWW), `block.rs` (`merge_block` -> `MergedBlock` with `Conflict`s), `conflict.rs`.
- `crates/bitacora-merge/tests/matcher_accuracy.rs`: synthesized edits over fixtures, 166 pages / 3413 pairs, precision = recall = 1.000.
- `docs/design/git-sync-merge.md` open question 3 updated.

## Decisions
- Merge crate uses its own LCS diff3 (no new diff dependency); only `uuid` + dev `proptest` added.
- `merge_metadata` in markdown works at the CanonicalView level (surgical edit functions are not available yet); serialization of merged blocks is BIT-US-0051.
- A conflicting field keeps the ours value; any content overlap keeps ours for the whole block text.
- SCHEDULED/DEADLINE are extracted into `MergeBlock.planning` (the serializer must re-place them after the title).

## Verification
`cargo fmt --all`, `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; `cargo test --workspace --locked` green (markdown 73 lib tests, merge 58 + accuracy 1); `cargo xtask check-deps` OK; `cargo deny check` ok.
