---
id: BIT-T-0052
type: task
title: Implement ignore rules and :hidden prefix matcher
status: backlog
priority: critical
parent: BIT-US-0027
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 3
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:37:38Z
---

## Description
Create `crates/bitacora-core/src/graph/ignore.rs` with a pure function `is_ignored(rel_path: &GraphPath, hidden: &[String]) -> bool` and `is_indexable_ext(ext) / is_parseable_ext(ext)`.

Re-implement the ignore rules from the documented behaviour in [[01-file-graph-layout]] §1.1 (Logseq reference for behaviour only: `deps/common/src/logseq/common/graph.cljs:66-113` `ignored-path?`/`get-files`, `deps/common/src/logseq/common/config.cljs:5-25` `:hidden`):
- ignore paths starting with `.`, `logseq/.recycle`, `logseq/bak`, `logseq/version-files`;
- exactly `logseq/graphs-txid.edn`, `logseq/pages-metadata.edn`;
- containing `/node_modules/`; ending with `.DS_Store`; any segment matching `^\.[^.]+`;
- extension whitelist `org markdown md edn json js css excalidraw tldr`; parser set `md markdown org edn css`;
- `:hidden`: prepend `/` to both pattern and path when missing, then plain prefix test (not a glob).

## Acceptance Criteria
- Table-driven unit tests covering every rule above, including `"/archived"` vs `"archived"` equivalence and `"../assets/archived"` never matching.
- `pages/archived.md` not hidden by `/archived`.
- No allocation-heavy regex per path (precompile or use string ops).

## Notes
Refs BIT-SP-0002.R2. [[01-file-graph-layout]] §1.1. ADR-015 (re-implemented from docs, no Logseq code copied/translated).
