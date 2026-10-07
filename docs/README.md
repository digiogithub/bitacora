# Bitacora knowledge base

Start with [[architecture]].

## Logseq analysis (reference: Logseq 0.10.15, file graphs)

| Page | Topic |
|---|---|
| [[01-file-graph-layout]] | Folder layout, `config.edn`, title ↔ file name encoding, journals, assets, rename/delete/backups |
| [[02-markdown-block-syntax]] | Outline grammar, properties (content vs metadata), refs, tasks, macros, serialization |
| [[03-parsing-indexing-search]] | Parse pipeline, data model, path-refs, search, query DSL |
| [[04-editor-outliner-operations]] | Editing model, outliner ops, write path, undo, keyboard shortcuts |
| [[05-git-and-apis]] | Git auto-commit, HTTP API, plugin API surface |

## Rust ecosystem

| Page | Topic |
|---|---|
| [[gpui-and-gpui-kit]] | GPUI fundamentals, GPUI Kit components, text-editing strategy |
| [[crate-stack]] | Crates and versions, workspace layout, CI |

## Bitacora design

| Page | Topic |
|---|---|
| [[architecture]] | Overview, crate map, ADR log, milestones |
| [[block-editor]] | Document model, ops, undo, byte-preserving serializer, GPUI editor |
| [[sqlite-index-schema]] | SQLite DDL, indexing pipeline, search, query DSL → SQL |
| [[git-sync-merge]] | Sync loop, git backend, block-aware 3-way merge, conflict UI |
| [[mcp-server]] | MCP over Streamable HTTP: tools, resources, security |

## User guide

See [[docs/user/README|user guide]]: [[whats-new-2.0]], [[upgrade-from-1x]], [[ui-tour]], [[graph-view-guide]], [[pando-setup]], [[ai-features]]. Release: [[release-process]], [[release-checklist-2.0]].

Backlog: gintrack project `BIT` (`.pmngr/`).
