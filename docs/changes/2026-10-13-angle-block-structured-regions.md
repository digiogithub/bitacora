---
created_at: 2026-10-13T12:00:00Z
updated_at: 2026-10-13T12:00:00Z
tags:
    - change
    - editor
    - ui
---
# Structured markup in special regions and EXPORT regions (BIT-T-0510, BIT-T-0511)

Part of [[angle-block-commands-gaps-plan]]; continues [[2026-10-07-special-blocks]].

## What changed

BIT-T-0510: `#+BEGIN_NOTE/TIP/IMPORTANT/CAUTION/WARNING/PINNED/QUOTE/CENTER`, consecutive `> ` lines and unknown
`#+BEGIN_<name>` regions now hold nested `BodyItem`s (paragraphs, lists, headings, code fences, quotes, nested regions)
instead of one inline layout. `VERSE` keeps one inline layout so line breaks survive. Unknown names are a new
`BodyItem::Container` rather than a code block. `SRC/EXAMPLE/EXPORT/QUERY/COMMENT` stay verbatim.

BIT-T-0511: `#+BEGIN_EXPORT <type>` renders as a code view with a localized label and html/latex highlighting
(Logseq renders sanitized HTML / KaTeX, `block.cljs:3262-3280`; read for behaviour only). ADR-035 records the divergence.

## Files and symbols

- `crates/bitacora-app/src/render/model.rs`: `Special` (+`Container`), `is_raw_region`, `OpenSpecial` (depth), `is_end_of`,
  `is_begin_of`, `BodyItem::{Quote(Vec), Callout{body}, Center(Vec), Container, Heading, List}`, `ListEntry`, `nested_items`,
  `region_item`, `list_marker`, `heading_marker`, `CodeBlock.export`, `ExportKind`.
- `crates/bitacora-app/src/views/block_view.rs`: `nested_element`, `structured_item`, `export_label`, `code_element` label.
- `crates/bitacora-app/src/print.rs`: `items_html` (recursive).
- `crates/bitacora-app/src/render/highlight.rs`: `html`/`xml`/`svg` and `latex`/`tex` languages.
- Locales `en/es/fr/zh.yml`: `page.export_html`, `export_latex`, `export_ascii`, `export_plain`.
- Fixture `fixtures/graphs/edge-cases/pages/nested-special-regions.md` (+ PROVENANCE, MANIFEST, snapshots in
  bitacora-markdown and bitacora-index) proves the parser/serializer round trip stays lossless.
- Docs: `docs/design/block-editor.md` section 8, `docs/architecture.md` ADR-035.

## Why

Logseq renders region bodies as full markup; flat text lost lists, headings and fences. Rendering never feeds back into
saved text.

## Verification

`cargo test -p bitacora-app --locked` (685 pass, new model tests for NOTE with list/fence/heading, QUOTE with NOTE,
`#+BEGIN_FOO` container, VERSE breaks, one export test per type); `cargo test --workspace --locked --exclude bitacora-app`
with the new snapshots; `cargo xtask fixtures verify`; clippy `-D warnings` on bitacora-app.
