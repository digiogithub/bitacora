---
id: BIT-T-0359
type: task
title: Path policy dispatch and EDN-aware config.edn 3-way merge
status: done
priority: high
parent: BIT-US-0052
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, bitacora-config, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T18:59:52Z
closed: 2026-10-06T18:59:52Z
---

## Description
`crates/bitacora-sync/src/merge/policy.rs`: `policy_for(path) -> Policy{Markdown, Config, TextLine, WholeFileKeepBoth, Binary, Ignore}` per [[git-sync-merge]] §5.1 (non-UTF-8 `.md` → Binary). `merge/edn.rs`: parse base/ours/theirs with `bitacora-config`'s comment-preserving EDN reader; per top-level key 3-way, nested maps recurse, sets union, vectors 3-way list; apply resulting changes as text edits on ours to keep comments; same-key divergence or parse failure → `Conflict::Config{key}` with ours in output.

## Acceptance Criteria
- Tests: disjoint key edits merged with comments preserved; same-key conflict; unparsable file → conflict.

## Notes
Story BIT-US-0052. Implements BIT-SP-0006.R17. See [[01-file-graph-layout]] (config.edn).
