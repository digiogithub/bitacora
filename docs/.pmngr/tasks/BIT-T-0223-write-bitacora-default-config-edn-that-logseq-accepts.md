---
id: BIT-T-0223
type: task
title: Write Bitacora default config.edn that Logseq accepts
status: done
parent: BIT-US-0099
milestone: BIT-M-0003
author: mcp
labels: [bitacora-config, compat]
estimate: 2
created: 2026-10-06T14:31:28Z
updated: 2026-10-06T19:13:24Z
closed: 2026-10-06T19:13:24Z
---

## Description
Write Bitacora's own default `config.edn` for new graphs as `crates/bitacora-config/assets/default-config.edn` (embedded via `include_bytes!`). It is authored by us: only keys/values (facts) needed for Logseq 0.10.15 to open the graph as a current file graph, e.g. `:meta/version 1`, `:file/name-format :triple-lowbar`, plus any further keys Bitacora wants to set explicitly. Do not copy text, comments or layout from Logseq's `src/resources/templates/config.edn`; any comments are our own. Byte-identity with Logseq's template / its MD5 default-config detection is a non-goal.

## Acceptance Criteria
- Decision recorded (note in [[01-file-graph-layout]] §2.3 referencing ADR-015).
- Test verifies the default config parses as EDN and contains the required keys (`:meta/version 1`, `:file/name-format :triple-lowbar`).
- Manual check: a graph created with it opens in Logseq 0.10.15 without a conversion prompt.

## Notes
BIT-SP-0002.R19. [[01-file-graph-layout]] §2.3. ADR-014, ADR-015 (Logseq's AGPL config template is not embedded).
