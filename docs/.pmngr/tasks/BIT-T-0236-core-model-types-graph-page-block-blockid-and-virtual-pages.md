---
id: BIT-T-0236
type: task
title: "Core model types: Graph, Page, Block, BlockId and virtual pages"
status: backlog
priority: high
parent: BIT-US-0098
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 3
created: 2026-10-06T14:31:34Z
updated: 2026-10-06T14:31:34Z
---

## Description
`crates/bitacora-core/src/model/{graph.rs,page.rs,block.rs}`:
- `Graph { root, config: EffectiveConfig, pages: HashMap<PageKey, Page>, files: HashMap<GraphPath, FileEntry>, diagnostics }`.
- `Page` fields as in the story; `PageOrigin { File, Referenced, Alias, NamespaceParent, Tag, Property }`; `file: None` for every non-File origin.
- `Block { id: BlockId /*session-stable, ADR-006*/, persisted_uuid: Option<Uuid>, page: PageKey, parent, order, raw span handle into bitacora-markdown doc }` — thin wrapper over the markdown crate's block tree, no duplication of text.
- Loading: `Graph::load(root) -> Graph` wiring scan → read → parse (bitacora-markdown) → derive_title → detect_journal → register pages.
Synchronous, no tokio (ADR-012).

## Acceptance Criteria
- Unit tests for page registration ordering, virtual page creation from refs, and that `Graph::load` performs no writes (temp dir with read-only permissions passes).
- Public API documented with rustdoc.

## Notes
Refs BIT-SP-0002.R12, BIT-SP-0002.R8. Block identity details belong to BIT-SP-0001.R7.
