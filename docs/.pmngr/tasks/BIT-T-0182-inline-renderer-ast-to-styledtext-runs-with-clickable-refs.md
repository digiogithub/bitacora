---
id: BIT-T-0182
type: task
title: "Inline renderer: AST to StyledText runs with clickable refs"
status: done
priority: high
parent: BIT-US-0074
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, rendering]
estimate: 5
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:44:23Z
started: 2026-10-06T18:28:35Z
closed: 2026-10-06T18:44:23Z
---

## Description
`crates/bitacora-app/src/render/inline.rs`: convert the `bitacora-markdown` inline AST (parse block body with `pulldown-cmark` for emphasis/code/links per ADR-003 plus Bitacora's ref scanner) into `InteractiveText` built from `StyledText` with `TextRun`s and highlight ranges: bold/italic/strike/inline code, links (open externally), `[[page]]` and `#tag` (theme link colour, click → navigate, Shift+click → right sidebar), `((uuid))` resolved through `IndexReader::block` to its title (dangling → raw text muted), `{{embed}}`/`{{query}}` → placeholder chip. Respect `:ui/show-brackets?`.

## Acceptance Criteria
- Unit tests on the AST→runs mapping (ranges and styles) for 15 representative blocks.
- `#[gpui::test]`: clicking a page ref run emits `Navigate(PageName)`.

## Notes
[[gpui-and-gpui-kit]] §2.3 (inline rendering), [[block-editor]] §8. ADR-003.
