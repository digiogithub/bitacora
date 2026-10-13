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

## Review fixes (BIT-US-0188)

- `render/model.rs`: `nested_items` and `Special::item` take a nesting depth; beyond `MAX_NEST_DEPTH` (32) the remaining lines are shown as one plain text item, so thousands of nested `#+BEGIN_QUOTE` lines or a very long `>>>>` line cannot overflow the stack.
- `render/model.rs`: same-name BEGIN/END depth counting moved into `RegionScan`, which skips lines inside ``` fences and raw regions (SRC, EXAMPLE, EXPORT, QUERY, COMMENT); a `#+END_QUOTE` inside a code body no longer closes the quote. Used by both the top-level scanner and `nested_items`.
- `editor/view.rs` (`replace_text_in_range`): the full-width `《〈` to `<` swap now also applies when the IME commits with an explicit range or over its preedit, evaluated against the text before the replaced range, as a single edit.
- `bitacora-config` `custom_commands`: a later definition of the same name overrides the earlier text, keeping first-seen order.
- Tests: `hostile_nesting_does_not_overflow_the_stack`, `end_line_inside_raw_bodies_does_not_close_the_quote` (model.rs), `full_width_angle_swap_also_applies_to_an_ime_commit_with_a_range` (slash_tests.rs), `custom_commands_last_definition_per_name_wins_in_first_seen_order` (config tests/load.rs).
