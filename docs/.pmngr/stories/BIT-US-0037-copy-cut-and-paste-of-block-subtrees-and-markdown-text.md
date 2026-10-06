---
id: BIT-US-0037
type: story
title: Copy, cut and paste of block subtrees and Markdown text
status: done
priority: high
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, clipboard, bitacora-core, bitacora-app]
estimate: 8
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T21:12:42Z
started: 2026-10-06T20:22:06Z
closed: 2026-10-06T21:12:42Z
---

## Description
As a user moving content between pages and apps, I want copy/cut/paste of blocks to keep the structure, and pasting Markdown lists from other tools to create a proper block tree, so that I never have to rebuild outlines by hand.

## Acceptance Criteria
- Copy of a selection writes plain Markdown (tab-indented from depth 0, `id::` stripped), HTML, and a private MIME payload with uuids.
- Cut = copy + `DeleteBlocks` in one transaction.
- Paste of the private payload: kept uuids after cut, fresh identities (no `id::`) after copy.
- Plain-text paste: `^\s*([-+*]|#+)\s+` → parsed block tree; blank-line paragraphs → sibling blocks; otherwise inline.
- Pasting blocks onto an empty edited block replaces it; `Mod+Shift+V` raw inline paste; CRLF normalised.
- `InsertBlocks { target, sibling, blocks, keep_uuids }` is a pure planner with fixture tests.

## Notes
Implements: BIT-SP-0004.R13.
See [[block-editor]] §7.5, [[04-editor-outliner-operations]] §3 (Paste, `paste.cljs:23-47`, `:101`, `:118-177`), §3.1 `insert-blocks`. ADR-006.
