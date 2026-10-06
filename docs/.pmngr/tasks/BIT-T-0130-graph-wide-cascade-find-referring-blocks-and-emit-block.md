---
id: BIT-T-0130
type: task
title: "Graph-wide cascade: find referring blocks and emit block edits"
status: done
parent: BIT-US-0082
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-index, rename]
estimate: 3
created: 2026-10-06T14:29:56Z
updated: 2026-10-06T19:30:10Z
closed: 2026-10-06T19:30:10Z
---

## Description
Collect every block referring to the renamed pages (page + namespace children + property-key refs) through a `RefLookup` trait defined in `crates/bitacora-core/src/lifecycle/ref_lookup.rs` (core must not depend on `bitacora-index`; the index implements the trait, and core ships an in-memory full-scan implementation used as fallback and in tests). Apply `rewrite_block_refs` per block and emit `Op::EditBlock` for changed blocks only, grouped per file, inside the rename transaction. Re-read files whose hash changed since indexing (ADR-011 pre-write check).

## Acceptance Criteria
- Rename with 50 referring blocks across 20 files changes only those blocks; all other bytes identical (assert via diff of tempdir).
- Full-scan and index-backed lookups return the same block set (test).
- Index after rename: zero refs to old key, all refs to new key.
- Whole cascade undoes in one step.

## Notes
BIT-SP-0002.R13. Dependency direction per AGENTS.md §2. [[sqlite-index-schema]], [[block-editor]].
