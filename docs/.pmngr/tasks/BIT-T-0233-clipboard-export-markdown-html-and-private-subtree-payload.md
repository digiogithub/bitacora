---
id: BIT-T-0233
type: task
title: "Clipboard export: Markdown, HTML and private subtree payload"
status: done
priority: high
parent: BIT-US-0037
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, clipboard]
estimate: 3
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
`crates/bitacora-core/src/clipboard.rs`: `export_blocks(&Graph, ids) -> ClipboardPayload { markdown, html, private }`. Markdown: top-level blocks re-indented from depth 0 with tabs, `id::` lines stripped; HTML: nested `<ul><li>` with rendered inlines; private: serde JSON (`application/x-bitacora-blocks`) with texts, depths, uuids and a `cut: bool` flag. App writes all formats via GPUI clipboard (falls back to text-only where multi-format is unavailable). `Mod+X` = copy + `DeleteBlocks`.

## Acceptance Criteria
- Unit tests for Markdown output (nested, multi-line, CRLF source → LF clipboard).
- Cut is a single undo step.

## Notes
Story BIT-US-0037. Implements BIT-SP-0004.R13. Logseq `paste.cljs:87-99`, `editor.cljs:1001-1079`.
