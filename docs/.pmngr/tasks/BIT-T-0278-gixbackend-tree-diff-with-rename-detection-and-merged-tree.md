---
id: BIT-T-0278
type: task
title: "GixBackend: tree diff with rename detection and merged tree writing"
status: done
priority: high
parent: BIT-US-0042
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, gix, backend]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:30:22Z
closed: 2026-10-06T17:30:22Z
---

## Description
`crates/bitacora-sync/src/backend/gix_trees.rs`: `diff_trees(a, b)` via `gix` tree-diff with rewrite tracking (`Rewrites { percentage: 0.5, .. }`), mapping to `TreeChange`; add a strong-signal pass: Deleted+Added `.md` pairs with identical non-empty `id::` sets are reported as `Renamed` even below 50%. `write_tree(base, edits)` via `repo.write_blob` + tree editor (`repo.edit_tree(base)` upsert/remove) returning the new tree id; no work-tree or index mutation.

## Acceptance Criteria
- Tests: rename with edits detected; id-set rename detection; written tree equals `git write-tree` of the same content.

## Notes
Story BIT-US-0042. Implements BIT-SP-0006.R6, BIT-SP-0006.R19.
