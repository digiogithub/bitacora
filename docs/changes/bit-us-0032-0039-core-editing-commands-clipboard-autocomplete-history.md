---
created_at: 2026-10-06T20:22:45.278584109Z
updated_at: 2026-10-06T20:22:45.278584109Z
tags:
    - change
    - core
    - editor
---
# BIT-US-0032..0039: core editing commands, clipboard, autocomplete data, undo history

Continues [[bitacora-full-development-plan]]; designs [[block-editor]] (new implementation notes section), [[04-editor-outliner-operations]]; earlier core: [[bit-us-0029-0062-core-editor-model-ops-and-command-queue]], [[bit-us-0063-0066-write-pipeline]].

## What changed (crates/bitacora-core, plus additive bitacora-markdown helpers)
- `editor/split.rs`: `Cmd::Enter/SplitBlock/InsertNewline/OutdentEmptyLast/MergeWithPrevious/MergeNext`, `enter_action`, typing `EditText`. Logseq rules: head keeps id::/collapsed::, tail left-trimmed, first child when children shown, insert-before at caret 0; merge into previous visible block, refusals, id adoption (both ids -> refused).
- `editor/outline.rs`: `MoveUpDown`, `CollapseBlocks`, `CollapseLevel`, `SetAllCollapsed`, `CycleMarker`, `SetMarker`, `ToggleDone`. `cmd.rs`: `plan_full` (`Planned` with cursor_after), logical outdent via `settings.rs` `EditorSettings`; fixed multi-block `move_blocks` indices.
- `editor/clipboard.rs`: export (text/html/private), `Workspace::cut_blocks`, `paste_private`, `Cmd::InsertBlocks`, `Cmd::PasteText`, `classify_paste`, `parse_outline`.
- `editor/complete.rs`: `CompletionProvider`, candidates, `complete_page`, `Cmd::EnsureUuid`, `Cmd::InsertBlockRef`, `Workspace::copy_block_ref`.
- `editor/history.rs` + `queue.rs`: `History`, `Request::Undo/Redo/SetSettings`, `Response::Undone/Redone/HistoryFailed/Done`, `CommandQueue::undo/redo/set_settings`; `tx.rs` `run` fills cursors and coalesce key.
- `model.rs`: `Origin.base/older` (`BaseBytes`), verbatim re-emission of older block bytes (Document::extend_source in bitacora-markdown), final-newline convention, `text_is_isolated` (unclosed fences refused by split/merge).
## Verification
cargo fmt, clippy workspace -D warnings clean; tests: editor_commands (40), editor_history (15 incl. 2 proptests, also run at 4000 cases), editor_conventions (3), existing core/markdown suites pass.
## Known limits
Undo after a write restores exact bytes for blocks within the 6 remembered versions; indent unit of a page that lost all nesting is re-detected. UI tasks remain open and have comments naming the core API.
