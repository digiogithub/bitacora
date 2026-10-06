---
id: BIT-US-0096
type: story
title: Assets paste/drop with Logseq naming and relative links
status: backlog
priority: medium
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, assets]
estimate: 5
created: 2026-10-06T14:30:53Z
updated: 2026-10-06T14:30:53Z
---

## Description
As a user, I want to paste or drop images and files into a block and have them saved and linked exactly as Logseq does, so that the same graph shows the attachments in both apps.

Logseq (`handler/editor.cljs:1392-1454`, `handler/assets.cljs:117-128`): files go to `<graph>/assets/<stem>_<epoch-ms>_<index><ext>` (stem: ` `, `%`, `/` → `_`, `_` runs collapsed; ext from `extname`, multi-dot fallback). The link is relative to the page file: `![<name>](../assets/…)` for image/audio/video/PDF, `[<name>](../assets/…)` otherwise; base `pages/_.md` if the page has no file yet. Links starting with `../assets`, `./assets`, `/assets`, `assets` are local (`^[./]*assets`); `@alias/…` links are preserved verbatim.

## Acceptance Criteria
- Pasting `Screen Shot 2024.png` at `1731580000000` into `journals/2025_11_14.md` creates `assets/Screen_Shot_2024_1731580000000_0.png` and inserts `![Screen Shot 2024.png](../assets/Screen_Shot_2024_1731580000000_0.png)`.
- Dropping `report 50%.docx` as the 2nd file into `pages/sub/x.md` → `assets/report_50_1731580000000_1.docx` and `[report 50%.docx](../../assets/report_50_1731580000000_1.docx)`.
- Clipboard image → `assets/image_<ms>_0.png`.
- Asset links render with resolution relative to the file and fallback to `<root>/assets`; `@alias/x.pdf` shown as unresolved link, text unchanged.
- Asset files are written atomically; `assets/` created on first use.

## Notes
Implements: BIT-SP-0002.R15
See [[01-file-graph-layout]] §5; [[04-editor-outliner-operations]]; [[block-editor]]. ADR-011.
