---
id: BIT-T-0224
type: task
title: create_graph scaffolding in core
status: backlog
parent: BIT-US-0099
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, lifecycle]
estimate: 2
created: 2026-10-06T14:31:28Z
updated: 2026-10-06T14:40:05Z
---

## Description
`crates/bitacora-core/src/graph/create.rs`: `fn create_graph(dir) -> Result<Graph>`: refuse non-empty dirs that are not graphs (no `logseq/config.edn`); create `pages/`, `journals/`, `logseq/.recycle/`, `logseq/custom.css` (empty), `logseq/config.edn` (Bitacora's own default config from BIT-T-0223, not Logseq's template), `pages/contents.md` = `-`, all via atomic writes, UTF-8 no BOM. Same resulting layout as Logseq's `handler/repo.cljs:40-124` (reference for behaviour only). Expose in `bitacora-cli` as `bitacora init <path>` and in the app "New graph" dialog.

## Acceptance Criteria
- Tempdir test asserts the exact tree and file bytes.
- Non-empty folder → error; existing graph → opened unchanged.
- Opening the result indexes `Contents` page.

## Notes
BIT-SP-0002.R19, R1. ADR-015 (no Logseq template embedded).
