---
id: BIT-EP-0004
type: epic
title: Graph model, config.edn and file layout
status: backlog
priority: critical
milestone: BIT-M-0002
author: mcp
labels: [core, compat]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:37:04Z
---

## Description
`bitacora-core` + `bitacora-config`: graph discovery and ignore rules (`:hidden`, dot-dirs, `logseq/bak`, `.recycle`), `config.edn` loading (global + graph merge) with comment-preserving edits, page title ↔ file name mapping for `:triple-lowbar` and legacy formats, page identity (lower-case + NFC), journals detection and naming, page/block domain model, aliases, namespaces.

## Acceptance Criteria
- Our own title↔path test vectors covering the documented cases in [[01-file-graph-layout]] §3.2 pass (verified black-box against Logseq, not copied from Logseq's test files).
- Journal detection matches Logseq for all supported title formats.
- Editing a config key preserves comments and formatting of the rest of `config.edn`.

## Notes
ADR-013. ADR-015 (no Logseq code/tests/templates copied; behaviour re-implemented from docs). See [[01-file-graph-layout]].
