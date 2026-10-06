---
id: BIT-T-0277
type: task
title: "GixBackend: status, unmerged entries, read_blob and merge_base"
status: backlog
priority: high
parent: BIT-US-0042
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, gix, backend]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
`crates/bitacora-sync/src/backend/gix_reads.rs` using `gix::open` (isolated in this module): `status()` via `repo.status()` iterator → dirty tracked/untracked paths respecting `.gitignore`, plus index entries with stages 1/2/3 (unmerged, e.g. after an external `git pull`); `read_blob(commit, path)` via `commit.tree().lookup_entry_by_path` → `object().data`; `merge_base(a, b)` via `repo.merge_base`. Detect in-progress operations (`.git/MERGE_HEAD`, `rebase-merge/`, `rebase-apply/`).

## Acceptance Criteria
- Tests comparing with `git status --porcelain=v2`, `git merge-base` and `git show` on fixture repos including an unmerged path.

## Notes
Story BIT-US-0042. Implements BIT-SP-0006.R6, BIT-SP-0006.R8. ADR-007.
