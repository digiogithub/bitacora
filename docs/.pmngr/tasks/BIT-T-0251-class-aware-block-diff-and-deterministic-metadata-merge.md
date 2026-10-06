---
id: BIT-T-0251
type: task
title: Class-aware block diff and deterministic metadata merge helpers
status: backlog
parent: BIT-US-0094
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, merge]
estimate: 3
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T14:32:01Z
---

## Description
Add `crates/bitacora-markdown/src/merge_helpers.rs`:
- `normalize_for_compare(block) -> CanonicalView` (content lines with whitespace/indent normalised, properties as an ordered map split by class, logbook CLOCK set).
- `classify_diff(base, ours, theirs) -> DiffClass { Identical, MetadataOnly, Content, IdentityConflict }` — property reorder and whitespace-only changes are MetadataOnly.
- `merge_metadata(base, ours, theirs, prefer: Side) -> Block`: last-writer-wins per metadata key (`prefer` = newer side), union of CLOCK lines sorted by start, `id` kept from whichever side has it; if both differ, keep ours and report `IdentityConflict` for de-dup by uuid. Output is produced via the surgical edit functions so untouched lines keep bytes.

## Acceptance Criteria
- Two versions differing only by `collapsed:: true` and an extra CLOCK line → `MetadataOnly`, merged block contains both CLOCK lines.
- Property reorder only → `MetadataOnly`.
- Different title text → `Content`.
- Proptest: merge is deterministic and idempotent (`merge(x,x,x) == x`).

## Notes
Part of BIT-US-0094. Implements BIT-SP-0001.R16. Consumed by [[git-sync-merge]] (BIT-EP-0012); open question §12.5 (SRS date formats) must be checked against `srs.cljs` before finalising.
