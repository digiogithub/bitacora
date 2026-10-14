---
created_at: 2026-10-14T11:03:28.805471281Z
updated_at: 2026-10-14T11:03:37.724107764Z
tags:
    - change
    - editor
    - ui
---
# Regions on the first line render without markers; refreshed admonition and quote look

Continues [[2026-10-13-angle-block-structured-regions]] and [[angle-block-commands-gaps-plan]] (BIT-US-0188).

## What changed

- Bug: a block whose first line opens a region (`- #+BEGIN_QUOTE`, `- #+BEGIN_NOTE`, ... — exactly what the `<` menu
  inserts) or a `> ` quote showed `#+BEGIN_X` as its title and the body/`#+END_X` as plain paragraphs. Only
  `#+BEGIN_QUERY` and ```` ``` ```` were special-cased. Now any opening line (fence, `#+BEGIN_<name>`, `>`) after the
  marker/priority prefix goes through the body path, so the block has no title and renders only the box.
- View: the empty title line is hidden whenever a non-text, non-widget item opens the block (`region_opens_block`,
  generalizes the old `code_first`).
- Look: admonitions are tinted cards with a tone outline, the kind's Lucide icon in a round badge (info, lightbulb,
  circle-alert, octagon-alert, triangle-alert, pin) beside a small localized title; quotes are an accent rule on a soft
  `hover` panel with `text_2` text. Palette stays accent/warn (design rule, test kept).

## Files and symbols

- `crates/bitacora-app/src/render/model.rs`: `opens_region`, `BlockModel::from_content` first-line branch; test
  `regions_opening_the_block_render_without_a_title_line`.
- `crates/bitacora-app/src/views/block_view.rs`: `callout_style` (returns `Glyph`), `glyph_icon`, `callout_element`,
  `quote_element`, `region_opens_block`; test `a_region_opening_the_block_hides_the_title_line`.
- `crates/bitacora-app/src/views/kit/icons.rs`: `Glyph::{Info, Lightbulb, OctagonAlert, TriangleAlert, Quote}`.
- `crates/bitacora-app/src/views/dims.rs`: `CALLOUT_BORDER`, `CALLOUT_BADGE_TINT`, `QUOTE_RULE`, `QUOTE_TINT`.
- `docs/design/block-editor.md` section 8.

## Why

Owner report: Logseq shows these blocks as visual boxes without begin/end markers; Bitacora showed raw markers for
the common `- #+BEGIN_X` shape. View-only change, saved text untouched.

## Verification

`cargo test -p bitacora-app --locked` (696 pass), clippy `-D warnings` on bitacora-app, and a real-window screenshot
(X11, temp XDG dirs) of a page with QUOTE, fence, NOTE, IMPORTANT, TIP, WARNING with list, CAUTION, PINNED, `>` quote
and a titled block with a NOTE: no markers shown, cards rendered. Commit 460dbf5.
