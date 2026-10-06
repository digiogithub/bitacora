---
id: BIT-T-0039
type: task
title: Block UUID assignment with diff-based carry-over
status: backlog
priority: high
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T15:11:55Z
---

## Description
`crates/bitacora-index/src/writer/carry_over.rs`: `fn assign_uuids(old: &[OldBlock], new: &[ParsedBlock], taken: impl Fn(&Uuid) -> Option<FileId>) -> Vec<(Uuid, UuidSource)>`.
1. Explicit `id::` → source 1 (if owned by another file → fresh UUIDv7 + `duplicate_block_id` diagnostic).
2. Patience diff (`similar` crate) over `(depth, content_hash)`; equal → inherit (source 2).
3. Replaced hunks: positional pairing when same depth and normalized edit similarity (`strsim::normalized_levenshtein` on `content`; old content comes from the file's previous `blocks.content` rows loaded into `OldBlock` before the replace, not from a snapshot table) >= 0.5 → inherit.
4. Remaining → UUIDv7 (source 0). Never assign one UUID twice.

## Acceptance Criteria
- Tests: edit sibling keeps UUID; insert block at top keeps all others; edit block itself by one word keeps UUID; swap with large rewrite gets new UUID; `id::` always wins.
- Proptest: no duplicate UUIDs in output.

## Notes
BIT-SP-0003.R12, ADR-006, ADR-017 (no `file_snapshots`). [[sqlite-index-schema]] §2.2–2.3; Logseq `diff-merge-uuids-2ways`.
