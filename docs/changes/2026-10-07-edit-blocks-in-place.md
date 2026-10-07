---
created_at: 2026-10-07T17:40:00Z
updated_at: 2026-10-07T17:40:00Z
tags:
    - change
    - ui
    - editor
---
# Edit blocks in place wherever they are shown (BIT-US-0168)

Continues [[bitacora-v2-plan]] (epic BIT-EP-0017). Tasks BIT-T-0500, BIT-T-0501, BIT-T-0502.

## What changed
A click on the text of a block drawn from the index (linked/unlinked references, the zoomed block or page of a sidebar item, query results, right-panel backlinks, Tasks titles) enters inline edit mode with the same `OutlineEditor` as the outline (caret at the click, completion, slash commands, planning chips). Links, tags and refs inside the text still navigate. Embeds were already editable (BIT-US-0104); inline `((uuid))` refs keep navigating on click (Logseq behaviour).

- `views/remote_edit.rs` (new): `RemoteEditors<V>` keeps one `OutlineEditor` per source page, created on the first click (page loaded into the core workspace through `editor::ensure_loaded`), or eagerly for blocks that have SCHEDULED/DEADLINE chips so the chip picker works at once. `BlockRef::locate` maps the index row to the core block: `id::` uuid first, else file position (pre-block aware) accepted only while core's text equals the indexed text. The `RowEdit` of the editor is reused with the fold arrow kept view-only and the bullet not zooming the hidden editor. The drawn row takes the block model from core so every view shows typing at once. The editor leaves edit mode if the caret moves to a block that is not on screen (Enter, arrows). External-change conflicts (`EditingConflict`) are forwarded to these editors.
- `Row.content` (index rows) identifies the block; `RowActions.activate` is the click hook for rows without an editor yet (`block_view.rs`).
- Hosts: `PageView` (ref rows, and rows of non-live views such as the sidebar via `set_remote_link`), `RightSidebar::set_session_link`, `QueryBlock::set_link`, `RightPanel` backlinks (now rows instead of one-line snippets), `TasksView` (title click edits, Shift+click opens in the sidebar, new "open" link).
- All writes are core `EditText` transactions: single writer, undoable, file-hash checks of core apply.

## Tests
`page_view::tests::clicking_a_reference_edits_the_source_block_in_place`, `planning_chips_of_a_reference_open_the_picker_and_rewrite_the_source`, and `views/remote_edit_tests.rs` (query results, tasks, backlinks, zoomed block): click, type, only the source block changes on disk, one undo restores the bytes. `cargo test -p bitacora-app --locked` and clippy `-D warnings` pass.

## Limits
Tasks cards place the caret at the end of the block (the card words the title itself). Enter/arrow navigation to blocks not shown ends edit mode.
