---
id: BIT-US-0053
type: story
title: Merge orchestration, persisted conflict state and marker-free guarantee
status: backlog
priority: critical
parent: BIT-EP-0012
milestone: BIT-M-0004
author: mcp
labels: [merge, sync, conflicts]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T15:10:56Z
---

## Description
As a user, I want a sync with unresolved conflicts to leave my work tree clean (my version kept, everything else merged), survive restarts and never contain conflict markers, so that I can keep working and resolve later.

## Acceptance Criteria
- `sync_merge` per [[git-sync-merge]] §4.4: merge-base, diff both sides with renames, per-path policy (pages via `bitacora_merge::merge_page`), merged tree via gix, merge commit when clean.
- Conflicted: work tree written via core writer, `refs/bitacora/pending-merge` + `.git/bitacora/merge-state.json` (conflict records §4.5), restored on restart; nothing pushed until resolved; final `Kind: resolve` merge commit.
- Remote moving while conflicted recomputes with the same base and re-applies resolutions by block identity (rerere memo keyed by content hashes).
- External marker regions and unmerged index entries are converted to base/ours/theirs and merged; a property test asserts no output ever contains `^(<<<<<<<|=======|>>>>>>>)( |$)` lines introduced by the merge.

## Notes
Implements: BIT-SP-0006.R8, BIT-SP-0006.R15, BIT-SP-0006.R21. See [[git-sync-merge]] §2.2, §2.5, §4.4–4.5, §5.4. ADR-008, ADR-016 (orchestration and state stay in `bitacora-sync`; page merge in `bitacora-merge`).
