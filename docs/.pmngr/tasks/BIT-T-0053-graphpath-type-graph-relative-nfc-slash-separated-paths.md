---
id: BIT-T-0053
type: task
title: "GraphPath type: graph-relative, NFC, slash-separated paths"
status: backlog
priority: critical
parent: BIT-US-0027
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:28:27Z
---

## Description
Add `crates/bitacora-core/src/graph/path.rs` with a newtype `GraphPath(String)`:
- `GraphPath::from_abs(root, abs)` strips the root, converts `\` to `/` (Windows), applies Unicode NFC (`unicode-normalization` crate), rejects paths escaping the root.
- `to_abs(root)` for IO; `file_body()` = base name with everything after the **last** dot removed (`graph_parser/util.cljs:204-209`); `ext()`; `dir()`.
- Mirrors `graph_parser/util.cljs:24-28` and `common/graph.cljs:36-42`.

## Acceptance Criteria
- Tests: `pages\sub\foo.md` → `pages/sub/foo.md`; NFD `Cafe\u{301}.md` → NFC `Café.md`; `Version 1.0.md` file_body `Version 1.0`; `a.tar.gz` file_body `a.tar`.
- `from_abs` of a path outside the root returns an error (no panic).

## Notes
Refs BIT-SP-0002.R9. [[01-file-graph-layout]] §3.7.
