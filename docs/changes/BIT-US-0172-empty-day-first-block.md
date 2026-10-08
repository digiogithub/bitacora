# BIT-US-0172: empty day accepts its first block from the journals feed

Continues [[2-0-x-owner-feedback-plan]].

## What changed
- `OutlineEditor::focus_first_block` (`crates/bitacora-app/src/editor/view.rs`): focuses the first
  block of the page; when core holds the page with no block at all (an empty day file, a
  placeholder) it first runs `Cmd::InsertChild` with an empty root block, then enters it.
- `views/journals.rs` `render_entry`: the empty-day text is clickable when the day has a core
  editor and calls `focus_first_block`.
- `views/page_view.rs` `Item::Empty`: same click handler on a live page.

## Why
A virtual today already carries core's empty first block (`Workspace::ensure_journal`). A day with
zero blocks (an existing empty file, placeholder page) rendered only static text, with no caret
target, so Enter/Tab had nothing to act on. The file is still only written once content exists.

## Verification
`cargo test -p bitacora-app --lib -- journals`: new tests
`empty_today_in_the_feed_gets_an_editor_with_core_first_block` (virtual today: type, Enter, Tab,
nothing created before typing) and `an_empty_day_file_starts_its_first_block_from_the_feed`.
