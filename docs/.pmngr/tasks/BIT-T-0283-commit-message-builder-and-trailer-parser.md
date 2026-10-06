---
id: BIT-T-0283
type: task
title: Commit message builder and trailer parser
status: done
priority: high
parent: BIT-US-0044
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, commit]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:48:56Z
closed: 2026-10-06T17:48:56Z
---

## Description
`crates/bitacora-sync/src/commit_msg.rs`: `CommitMessage { kind: CommitKind{Auto, Manual(String), Merge, Resolve, Agent{client}, Migrate}, device, pages: Vec<RepoPath>, notes: Vec<String> }` → text with subject ≤ 72 chars (`bitacora: edit 3 pages (journals/2026_10_06, Project X, Ideas)` with page titles derived from paths via core naming, ellipsis when long) and trailers `Bitacora-Device`, `Bitacora-Kind`, `Bitacora-Pages` (`; `-separated), `Bitacora-Agent`. `parse(text) -> Option<CommitMessage>` for squash and history views.

## Acceptance Criteria
- Golden tests incl. long page lists, unicode titles, round-trip parse.

## Notes
Story BIT-US-0044. Implements BIT-SP-0006.R2.
