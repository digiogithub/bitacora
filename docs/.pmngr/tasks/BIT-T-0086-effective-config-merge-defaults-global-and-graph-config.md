---
id: BIT-T-0086
type: task
title: "Effective config merge: defaults, global and graph config"
status: backlog
priority: critical
parent: BIT-US-0056
milestone: BIT-M-0002
author: mcp
labels: [bitacora-config, compat]
estimate: 2
created: 2026-10-06T14:28:54Z
updated: 2026-10-06T14:28:54Z
---

## Description
`crates/bitacora-config/src/effective.rs`: `load_effective(graph_root, global_path: Option<&Path>) -> (EffectiveConfig, Vec<ConfigDiagnostic>)`.
- Built-in defaults (`state.cljs:335-345`): `:feature/enable-search-remove-accents? true`, `:ui/auto-expand-block-refs? true`, `:file/name-format :legacy`.
- Merge per `merge-configs` (`state.cljs:350-383`): later wins; when both values are maps, shallow-merge.
- Global config `~/.logseq/config/config.edn` is optional and read-only (never written); path resolution via `dirs` home dir, overridable for tests.
- On graph-config parse error: diagnostic + use defaults ⊕ global.

## Acceptance Criteria
- Tests for the three scenarios of BIT-SP-0002.R3 (legacy default, shallow map merge, duplicate keys).
- No write ever issued to the global config path (assert via read-only temp dir).

## Notes
Refs BIT-SP-0002.R3. [[01-file-graph-layout]] §2.1.
