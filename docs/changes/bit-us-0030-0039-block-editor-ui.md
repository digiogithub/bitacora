---
created_at: 2026-10-06T21:15:03.396839289Z
updated_at: 2026-10-06T21:15:03.396839289Z
tags:
    - change
    - ui
    - editor
    - bitacora-app
    - bitacora-core
---
# BIT-US-0030..0039 and BIT-T-0344: block editor UI

Continues [[bitacora-full-development-plan]]; designs [[block-editor]] (new "Implementation notes: editor UI"), [[04-editor-outliner-operations]], spike [[bit-us-0040-0060-0072-block-editor-spike]], core commands [[bit-us-0032-0039-core-editing-commands-clipboard-autocomplete-history]], external reload [[bit-us-0068-0069-0070-external-reload-merge-conflict]].

## What changed
- core: `crates/bitacora-core/src/editor/projection.rs` (`EditProjection`, `HiddenKeys`): hides id::, collapsed::, boolean heading::, built-in props, LOGBOOK and config `:block-hidden-properties`; hidden lines return at their anchor (number of visible lines before them); `visible_to_full` / `full_to_visible` offset maps. 10 tests.
- app, new tree `crates/bitacora-app/src/editor/`: `view.rs` `OutlineEditor` (snapshot, rows, selection, zoom, edit buffer, flush, commands, clipboard, undo, completion state, `EntityInputHandler`), `element.rs` (`BlockTextElement`, edit content, conflict bar, completion popup, `wrap`), `actions.rs` (outliner:: actions, key contexts, platform bindings), `outline.rs`, `completion.rs`, `autopair.rs`, `html.rs`, `style.rs`, `row.rs` (`RowEdit`), moved `buffer.rs`/`layout.rs`/`text_ops.rs` from the spike (spike files re-export), `tests.rs` (50+ keystroke-level `#[gpui::test]`).
- render: `render/inline.rs` `TextLayout::source_offset` + `SrcSeg` map, `layout_lines_at`/`layout_line_at`; `render/model.rs` `CodeBlock::source_offset`, absolute offsets for title/paragraph/quote lines.
- views: `block_view.rs` (`RowActions.edit`, click-to-caret wrappers, checkbox/bullet/arrow hooks, selection highlight), `page_view.rs` (`set_session_link`, `enter_live`, editor events, zoom breadcrumb, key-context wrapper), `journals.rs` (editor per day), `main_view.rs`, `workspace.rs` (session link and `SessionEvent::EditingConflict` to panes), `session.rs`.
- keymap: `assets/keymaps/default.json` outliner sections and `!BlockEditor` on the global chords; `keymap.rs` `load_with_user_report`; `app.rs` loads `<config>/keymap.json` and toasts ignored entries; `paths.rs` `keymap_file`.

## Why
Epic BIT-EP-0007: edit pages through core (single writer, undo, byte-exact files) with Logseq behaviour. Rows come from the core snapshot while the session is live; the index keeps header and references.

## Verification
cargo fmt, clippy --workspace --all-targets --locked -D warnings, typos, xtask check-deps, machete clean. `cargo test --workspace --locked --no-fail-fast`: 1371+ passed, 0 failed. Visual: Xvfb + lavapipe with XTest clicks/keys on a temp copy of logseq-docs (screenshots viewed): click-to-caret, hidden id::/collapsed:: byte-exact on disk after edit, Enter split + Tab, selection highlight, undo with caret restore, checkbox -> DONE, arrow expand, bullet zoom, `[[` popup.

## Limits / follow-ups
IME not exercised on real input methods (BIT-US-0031 in_review, BIT-T-0165). GPUI clipboard has no HTML/custom MIME entries: private payload travels as text metadata, HTML export not provided, HTML paste converted from text. `Route::Block` pages and the right sidebar stay read-only; no Up/Down across journal days; no mouse-drag block selection; popup is placed under the block; a page without a file and without a core copy is read-only (a `Request::OpenPage` is needed to start typing into placeholder pages).
