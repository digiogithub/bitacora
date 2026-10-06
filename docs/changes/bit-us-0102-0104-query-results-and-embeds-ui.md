---
created_at: 2026-10-06T22:20:50.064605067Z
updated_at: 2026-10-06T22:20:50.064605067Z
tags:
    - change
    - app
    - query
    - embed
---
# BIT-US-0102 / BIT-US-0104: live query results and in-place embeds

Continues [[bitacora-full-development-plan]]; builds on [[changes/bit-us-0101-0103-query-dsl-and-datalog.md]], [[changes/bit-us-0030-0039-block-editor-ui.md]], [[block-editor]], [[sqlite-index-schema]].

## What changed (crates/bitacora-app)
- `render/widget.rs`: `Widget::{Query,Embed}`, `QuerySpec`, `QueryProps` (query-table / query-properties / query-sort-by / query-sort-desc), `detect_line` (sole `{{query}}`/`{{embed}}` macro, also triple braces). `render/model.rs`: `BodyItem::Widget`; `#+BEGIN_QUERY` (also on the first line) becomes an advanced widget.
- `render/query/mod.rs` (`run`, `Scope`, `Body`, `Output`, `Failure`; routes simple vs advanced, caps 500 rows) and `render/query/table.rs` (columns, query-properties selection, numeric/text sort, blanks last).
- `render/embed/mod.rs`: `Chain` cycle guard + depth limit (default 5).
- `views/widgets/{mod,query_block,embed_block}.rs`: registry of widget entities, `prepare` (called by page view and journals before drawing a row), `QueryBlock` (grouped list with breadcrumbs, table, column picker, collapse, error/unsupported/warning boxes, 300 ms debounced refresh on relevant IndexEvents while on screen, stale re-run otherwise), `EmbedBlock` (nested `OutlineEditor` zoomed to the block, breadcrumb links, collapse, read-only index fallback, reload on source-file IndexEvent).
- Minimal edits: `views/block_view.rs` (`RowActions.widgets`, widget-only rows), `page_view.rs`/`journals.rs` (`row_widgets`/prepare), `workspace.rs` (index events to widgets), locale `widgets.en.yml`.

## Verification
fmt, clippy `--workspace --all-targets --locked -D warnings` clean, 16 new tests (widget detection, table model, embed guards, gpui tests incl. editing in an embed changes only the source file, refresh on index event, depth limit). Xvfb+lavapipe+XTest screenshots on a temp copy of logseq-docs: list, table, picker, sort, advanced, error box, block/page embed, circular embed, live refresh on external file change, editing in an embed.

## Notes
Embed depth limit is a constant (no config key). Block embeds need the block's `id::` in core's copy of the page; otherwise they stay read-only. Queries inside query results or references render as source text.
