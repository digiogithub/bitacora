---
id: BIT-US-0069
type: story
title: Block-level 3-way merge of external edits into dirty pages
status: done
priority: high
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, merge, bitacora-core]
estimate: 8
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T19:48:20Z
started: 2026-10-06T19:42:38Z
closed: 2026-10-06T19:48:20Z
---

## Description
As a user running Logseq and Bitacora on the same graph, I want edits to different blocks of the same page to merge automatically, so that I only get asked when both sides really changed the same block.

## Acceptance Criteria
- `merge3(base, theirs, ours) -> MergeOutcome::{Clean(Vec<Op>), Conflict(Vec<BlockConflict>)}` in `bitacora-core::external`, implemented as a thin adapter over `bitacora_merge::merge_page` from the `crates/bitacora-merge` crate (ADR-016); core contains no separate merge algorithm.
- base = `parse(disk.bytes)` where `disk.bytes` is the in-memory merge base (last bytes read/written for that file, `HashMap<FileId, Arc<[u8]>>` / `DiskSnapshot`; no `file_snapshots` table, ADR-017), theirs = `parse(new bytes)`, ours = in-memory page; alignment reused from the reload story / `bitacora-merge` matcher.
- No base available (after restart): no pending local edits → plain reload; pending local edits → conflict path with a 2-way per-block diff in the conflict notice (BIT-US-0070) (ADR-017).
- Disjoint text edits, adds at different positions, deletes of untouched blocks, and moves that do not cross merge cleanly; theirs' changes are applied as a non-undoable "External change" transaction and the page is written normally.
- Same-block edits, edit-vs-delete, and conflicting moves produce a conflict; no file write; no conflict markers ever written.
- Merge matrix tests with fixture triples (base/ours/theirs/expected), shared with the golden matrix of `bitacora-merge`.

## Notes
Implements: BIT-SP-0005.R15.
See [[block-editor]] §6.2 (MVP may ship detection + banner only; this story completes the merge), [[git-sync-merge]] for the shared block-aware merge used by sync (coordinate with BIT-EP-0012). ADR-008, ADR-009, ADR-011, ADR-016, ADR-017.
Dependency: this story (M2) needs the `bitacora-merge` page merge core (BIT-US-0049, BIT-US-0050, BIT-US-0051) before BIT-EP-0012's M3 git work.
