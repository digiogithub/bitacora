---
id: BIT-SP-0004
type: spec
title: Block editing and outliner operations
status: backlog
author: mcp
labels: [editor, core]
created: 2026-10-06T14:21:35Z
updated: 2026-10-06T21:14:48Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor
        - crates/bitacora-app/src/editor/element.rs#BlockTextElement
        - crates/bitacora-app/src/render/inline.rs#TextLayout::source_offset
        - crates/bitacora-app/src/views/block_view.rs#text_element_owned
      tests:
        - crates/bitacora-app/src/editor/tests.rs#clicking_text_enters_edit_mode_at_the_mapped_source_offset
        - crates/bitacora-app/src/editor/tests.rs#a_real_mouse_click_on_a_row_starts_editing
        - crates/bitacora-app/src/editor/tests.rs#arrow_keys_cross_block_boundaries_keeping_the_goal_x
        - crates/bitacora-app/src/editor/tests.rs#very_long_blocks_are_selected_instead_of_edited
        - crates/bitacora-app/src/render/inline.rs#display_offsets_map_back_to_source_offsets
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/projection.rs#EditProjection
        - crates/bitacora-core/src/editor/projection.rs#HiddenKeys
      tests:
        - crates/bitacora-core/src/editor/projection.rs#untouched_text_round_trips_for_odd_shapes
        - crates/bitacora-core/src/editor/projection.rs#hidden_lines_stay_after_the_title_when_text_is_added
        - crates/bitacora-core/src/editor/projection.rs#offset_round_trip_over_every_boundary
        - crates/bitacora-app/src/editor/tests.rs#hidden_properties_are_not_shown_and_return_byte_exact
        - crates/bitacora-app/src/editor/tests.rs#page_load_never_rewrites_untouched_blocks
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::flush
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::schedule_flush
        - crates/bitacora-app/src/editor/view.rs#text_diff
      tests:
        - crates/bitacora-app/src/editor/tests.rs#typing_commits_after_the_debounce_and_an_untouched_buffer_makes_no_op
        - crates/bitacora-app/src/editor/tests.rs#escape_flushes_and_selects_the_block
        - crates/bitacora-app/src/editor/tests.rs#ime_composition_keeps_enter_and_tab_away_from_the_outliner
  R4:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/model.rs#Page
        - crates/bitacora-core/src/editor/model.rs#Origin
        - crates/bitacora-core/src/editor/workspace.rs#Workspace
      tests:
        - crates/bitacora-core/tests/editor_ops.rs#untouched_page_serializes_identically_and_blocks_are_clean
        - crates/bitacora-core/tests/editor_ops.rs#same_depth_move_keeps_blocks_clean_and_moves_bytes
        - crates/bitacora-core/tests/editor_props.rs#every_fixture_page_roundtrips_through_the_model
    verified: {rev: "sha256:00fbaa7708439184", commit: 5580d94bce79b0a9fb12e95d667c3d435af37b17, at: 2026-10-06T19:56:30Z, by: claude}
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/op.rs#Op
        - crates/bitacora-core/src/editor/tx.rs#Workspace::commit
        - crates/bitacora-core/src/editor/cmd.rs#plan
        - crates/bitacora-core/src/editor/cmd.rs#plan_full
        - crates/bitacora-core/src/editor/history.rs#History
      tests:
        - crates/bitacora-core/tests/editor_ops.rs
        - crates/bitacora-core/tests/editor_props.rs#random_commands_invert_to_the_exact_original_bytes
        - crates/bitacora-core/tests/editor_props.rs#raw_text_ops_roundtrip
        - crates/bitacora-core/tests/editor_commands.rs
        - crates/bitacora-core/tests/editor_history.rs#random_semantic_commands_then_undo_all_restore_the_original_bytes
    verified: {rev: "sha256:54aaace5592a3f58", commit: 5580d94bce79b0a9fb12e95d667c3d435af37b17, at: 2026-10-06T19:56:30Z, by: claude}
  R6:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/split.rs#split_block
        - crates/bitacora-core/src/editor/split.rs#enter_action
        - crates/bitacora-core/src/editor/split.rs#outdent_empty_last
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::on_new_block
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#split_in_the_middle_left_trims_the_tail
        - crates/bitacora-core/tests/editor_commands.rs#caret_at_zero_inserts_an_empty_block_before_and_keeps_the_caret
        - crates/bitacora-core/tests/editor_commands.rs#enter_on_empty_last_child_outdents
        - crates/bitacora-core/tests/editor_commands.rs#shift_enter_adds_a_continuation_line
        - crates/bitacora-app/src/editor/tests.rs#enter_splits_at_the_caret_and_backspace_at_start_merges
        - crates/bitacora-app/src/editor/tests.rs#enter_on_an_empty_last_child_outdents_and_shift_enter_adds_a_line
        - crates/bitacora-app/src/editor/tests.rs#enter_inside_a_page_ref_jumps_past_the_brackets_instead_of_splitting
  R7:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/split.rs#merge_with_previous
        - crates/bitacora-core/src/editor/split.rs#merge_next
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::on_delete_backward
        - crates/bitacora-app/src/editor/autopair.rs#on_backspace
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#backspace_merges_into_previous_and_adopts_children
        - crates/bitacora-core/tests/editor_commands.rs#merge_refused_when_both_have_children_or_first_block
        - crates/bitacora-core/tests/editor_commands.rs#delete_pulls_in_next_sibling_or_first_child
        - crates/bitacora-app/src/editor/tests.rs#delete_at_the_end_pulls_the_next_block_and_refusals_change_nothing
        - crates/bitacora-app/src/editor/tests.rs#autopair_inserts_skips_and_deletes_pairs
  R8:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/editor/split.rs#merged_text]
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#referenced_block_identity_survives_a_backspace_merge
        - crates/bitacora-core/tests/editor_commands.rs#merge_refused_when_both_blocks_have_ids
  R9:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/cmd.rs#indent
        - crates/bitacora-core/src/editor/cmd.rs#outdent
        - crates/bitacora-core/src/editor/settings.rs#EditorSettings
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::structural
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#logical_outdenting_keeps_following_siblings
        - crates/bitacora-core/tests/editor_commands.rs#settings_come_from_config_edn
        - crates/bitacora-core/tests/editor_ops.rs
        - crates/bitacora-app/src/editor/tests.rs#tab_shift_tab_and_alt_shift_arrows_restructure_and_keep_the_caret
  R10:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/outline.rs#move_up_down
        - crates/bitacora-core/src/editor/cmd.rs#move_blocks
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::on_move_up
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#move_up_down_swaps_and_crosses_parents
        - crates/bitacora-core/tests/editor_commands.rs#move_up_down_works_on_selections_and_rejects_gaps
        - crates/bitacora-core/tests/editor_commands.rs#moved_clean_blocks_keep_their_bytes
        - crates/bitacora-app/src/editor/tests.rs#tab_shift_tab_and_alt_shift_arrows_restructure_and_keep_the_caret
  R11:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/outline.rs#collapse_blocks
        - crates/bitacora-core/src/editor/outline.rs#collapse_level
        - crates/bitacora-core/src/editor/outline.rs#set_all_collapsed
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::toggle_row
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::collapse_action
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#collapse_ignores_leaves_and_expand_restores_bytes
        - crates/bitacora-core/tests/editor_commands.rs#page_level_collapse_goes_one_level_at_a_time
        - crates/bitacora-app/src/editor/tests.rs#collapse_and_expand_persist_collapsed_true_and_restore_the_bytes
        - crates/bitacora-app/src/editor/tests.rs#page_level_collapse_acts_when_nothing_is_edited_and_t_o_toggles_all
        - crates/bitacora-app/src/editor/tests.rs#row_callbacks_zoom_fold_and_toggle_done
  R12:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/cmd.rs#plan_full
        - crates/bitacora-core/src/editor/clipboard.rs#selection_trees
        - crates/bitacora-app/src/editor/view.rs#Selection
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::move_selection
        - crates/bitacora-app/src/editor/outline.rs#Outline::top_level
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#bulk_operations_on_a_selection_are_one_transaction
        - crates/bitacora-app/src/editor/tests.rs#escape_selects_shift_arrows_extend_and_bulk_operations_use_one_transaction
        - crates/bitacora-app/src/editor/tests.rs#selection_delete_enter_and_select_all
        - crates/bitacora-app/src/editor/tests.rs#ctrl_a_in_selection_mode_selects_the_parent
  R13:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/clipboard.rs
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::paste
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::write_blocks
        - crates/bitacora-app/src/editor/html.rs#html_to_markdown
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#copy_exports_tab_indented_markdown_without_ids
        - crates/bitacora-core/tests/editor_commands.rs#cut_deletes_in_one_transaction_and_paste_keeps_ids
        - crates/bitacora-core/tests/editor_commands.rs#paste_after_copy_gets_fresh_identity
        - crates/bitacora-core/tests/editor_commands.rs#text_paste_classification
        - crates/bitacora-core/tests/editor_commands.rs#pasting_a_markdown_outline_builds_a_tree_and_replaces_an_empty_target
        - crates/bitacora-app/src/editor/tests.rs#copy_cut_and_paste_of_block_subtrees
        - crates/bitacora-app/src/editor/tests.rs#pasting_a_markdown_list_while_editing_creates_blocks_and_plain_text_goes_inline
        - crates/bitacora-app/src/editor/html.rs#nested_lists_become_tab_indented_bullets
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/outline.rs#cycle_marker
        - crates/bitacora-core/src/editor/settings.rs#Workflow
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::on_cycle_marker
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::toggle_done
      tests:
        - crates/bitacora-core/tests/editor_commands.rs#cycle_marker_follows_the_preferred_workflow
        - crates/bitacora-core/tests/editor_commands.rs#markers_keep_priority_and_properties_and_skip_empty_blocks
        - crates/bitacora-core/tests/editor_commands.rs#toggle_done_unchecks_to_the_workflow_start
        - crates/bitacora-app/src/editor/tests.rs#ctrl_enter_cycles_the_task_marker_and_the_checkbox_toggles_done
  R15:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/complete.rs#complete_page
        - crates/bitacora-core/src/editor/complete.rs#page_candidates
        - crates/bitacora-core/src/editor/lifecycle.rs#Workspace::open_page
        - crates/bitacora-app/src/editor/completion.rs
        - crates/bitacora-app/src/editor/autopair.rs
      tests:
        - crates/bitacora-core/tests/editor_history.rs#candidates_exclude_current_page_self_and_ancestors_and_offer_new_page
        - crates/bitacora-core/tests/editor_history.rs#choosing_a_new_page_creates_a_virtual_page_on_demand
        - crates/bitacora-app/src/editor/tests.rs#page_and_tag_completion_insert_the_right_text_and_navigate_with_the_keyboard
        - crates/bitacora-app/src/editor/tests.rs#autocomplete_context_wins_over_the_block_editor_context
        - crates/bitacora-app/src/editor/completion.rs#page_trigger_with_an_automatic_closer
        - crates/bitacora-app/src/editor/autopair.rs#opening_brackets_insert_the_pair
  R16:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/complete.rs#insert_block_ref
        - crates/bitacora-core/src/editor/complete.rs#ensure_uuid
        - crates/bitacora-core/src/editor/complete.rs#Workspace::copy_block_ref
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::accept_completion
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::copy_block_ref
      tests:
        - crates/bitacora-core/tests/editor_history.rs#block_ref_choice_adds_the_id_in_the_same_transaction
        - crates/bitacora-core/tests/editor_history.rs#copy_block_ref_persists_the_id_once
        - crates/bitacora-app/src/editor/tests.rs#block_reference_completion_writes_the_id_in_the_same_undo_step
        - crates/bitacora-app/src/editor/tests.rs#copy_block_ref_and_embed_persist_an_id_only_for_referenced_blocks
  R17:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/history.rs#History
        - crates/bitacora-core/src/editor/model.rs#Origin
        - crates/bitacora-core/src/queue.rs#CommandQueue::undo
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::history
      tests:
        - crates/bitacora-core/tests/editor_history.rs#undo_through_the_writer_restores_the_file_bytes
        - crates/bitacora-core/tests/editor_history.rs#random_semantic_commands_then_undo_all_restore_the_original_bytes
        - crates/bitacora-core/tests/editor_history.rs#undo_stops_with_a_notice_when_an_external_change_removed_the_target
        - crates/bitacora-core/tests/editor_conventions.rs
        - crates/bitacora-app/src/editor/tests.rs#undo_and_redo_restore_the_text_the_caret_and_the_bytes
  R18:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/editor/history.rs#History::push]
      tests:
        - crates/bitacora-core/tests/editor_history.rs#typing_coalesces_within_the_gap_and_splits_after_it
        - crates/bitacora-core/tests/editor_history.rs#word_boundary_after_a_pause_and_structural_ops_break_the_run
  R19:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::zoom_to
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::crumbs
        - crates/bitacora-app/src/views/page_view.rs#PageView::render_header
      tests:
        - crates/bitacora-app/src/editor/tests.rs#zoom_in_re_roots_the_view_with_a_breadcrumb_and_changes_no_file
        - crates/bitacora-app/src/editor/tests.rs#alt_arrows_zoom_and_ctrl_semicolon_toggles_all
        - crates/bitacora-app/src/editor/tests.rs#row_callbacks_zoom_fold_and_toggle_done
  R20:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/editor/view.rs#OutlineEditor::replace_and_mark_text_in_range
        - crates/bitacora-app/src/editor/element.rs#BlockTextElement
        - crates/bitacora-app/src/editor/buffer.rs#BlockBuffer::ime_replace_and_mark
      tests:
        - crates/bitacora-app/src/editor/tests.rs#ime_composition_keeps_enter_and_tab_away_from_the_outliner
        - crates/bitacora-app/src/editor/buffer.rs#ime_composition_replaces_marked_text_and_is_one_undo_step
        - crates/bitacora-app/src/editor/buffer.rs#marked_selection_is_relative_to_the_composition
  R21:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/editor/actions.rs
        - crates/bitacora-app/assets/keymaps/default.json
        - crates/bitacora-app/src/keymap.rs#load_with_user_report
        - crates/bitacora-app/src/paths.rs#AppDirs::keymap_file
      tests:
        - crates/bitacora-app/src/editor/tests.rs#every_default_binding_is_valid_and_user_overrides_are_validated
        - crates/bitacora-app/src/editor/tests.rs#autocomplete_context_wins_over_the_block_editor_context
        - crates/bitacora-app/src/editor/tests.rs#text_keys_inside_the_edited_block
---

## Purpose
Users edit notes block by block with Logseq's keyboard model, with reliable undo and minimal on-disk changes.

## Scope
Document model, `Op` set and transactions, undo/redo, edit-mode text handling, keyboard commands, autocomplete, selection, clipboard, rendering. Source: [[block-editor]], [[04-editor-outliner-operations]]. Implemented by BIT-EP-0007 and BIT-EP-0006.

## Requirements

### BIT-SP-0004.R1 — Single-block raw-text editing with click-to-caret and cross-block navigation

The editor SHALL edit exactly one block at a time as raw multi-line Markdown while all other blocks are rendered. Entering edit mode by clicking rendered text SHALL place the caret at the source offset of the clicked glyph (clicks on hidden markup such as `[[`/`]]` snap to the nearest offset). Up on the first visual row / Down on the last row SHALL move to the previous/next visible block keeping the x position; Left at offset 0 / Right at the end SHALL move to the end/start of the neighbouring visible block. Blocks longer than 10,000 characters SHALL be selected instead of entering edit mode.

#### Scenario: Click places caret inside a page reference
- GIVEN a rendered block whose source is `Meet [[Alice]] today`
- WHEN the user clicks between the `l` and `i` of the rendered word `Alice`
- THEN the block enters edit mode showing `Meet [[Alice]] today`
- AND the caret is at byte offset 9 (after `[[Al`)

#### Scenario: Down on last row moves to next visible block
- GIVEN block A `first` is being edited with the caret at column 3 and block B `second line` follows (A has collapsed children)
- WHEN the user presses `Down`
- THEN A's buffer is flushed, B enters edit mode with the caret at column 3, and collapsed children of A are skipped

#### Scenario: Left at offset 0 moves to previous block end
- GIVEN blocks `alpha` and `beta`, editing `beta` with caret at 0
- WHEN the user presses `Left`
- THEN `alpha` is edited with the caret at offset 5

### BIT-SP-0004.R2 — Hidden built-in properties are projected out of the edit buffer and re-inserted at their original positions

The editor SHALL remove hidden built-in property lines (`id`, `custom-id`, `collapsed`, boolean `heading`, `background-color`, `created-at`, `updated-at`, `last-modified-at`, `query-*`, `logseq.order-list-type`, `ls-type`, `hl-*`, `logseq.macro-*`, config `:block-hidden-properties`) and the `:LOGBOOK:` drawer from the edit buffer, and SHALL re-insert them verbatim at their original anchors (same order and position, clamped) when the buffer is committed. User properties such as `foo:: bar` SHALL stay visible and editable.

#### Scenario: id and collapsed lines survive a title edit
- GIVEN a block stored as
  ```
  - Buy milk
    id:: 6512a1b2-0000-4000-8000-000000000001
    collapsed:: true
    note:: fresh
  ```
- WHEN the user edits it, sees `Buy milk\nnote:: fresh`, and changes the first line to `Buy oat milk`
- THEN the committed text is `Buy oat milk\nid:: 6512a1b2-0000-4000-8000-000000000001\ncollapsed:: true\nnote:: fresh`
- AND the hidden lines keep their original order and position

#### Scenario: LOGBOOK drawer is hidden but preserved
- GIVEN a block `DONE write report` followed by a `:LOGBOOK:` … `:END:` drawer
- WHEN the user edits the title to `DONE write final report`
- THEN the drawer lines are written back unchanged after the title line

### BIT-SP-0004.R3 — Edit buffer is flushed on idle debounce and before any command, undo, navigation or blur

The editor SHALL commit the active edit buffer to the model (as one `EditText`/`SetText`) 500 ms after the last keystroke, and immediately on blur, `Esc`, block navigation, window deactivation, and before executing any structural command, undo or redo. A commit whose text equals the model text SHALL produce no op. No commit SHALL happen while an IME composition is active.

#### Scenario: Idle debounce commits typing
- GIVEN block `hello` is being edited
- WHEN the user types ` world` and stops typing for 500 ms
- THEN the model text becomes `hello world` and the page is marked dirty

#### Scenario: Structural command flushes first
- GIVEN the user typed `abc` into an empty block 100 ms ago
- WHEN the user presses `Tab`
- THEN the text `abc` is committed before the indent `Move` is applied, and both are reflected in the file

#### Scenario: Unchanged buffer produces no op
- GIVEN block `x` entered edit mode and the user pressed `Esc` without typing
- THEN no transaction is recorded and the page is not marked dirty

### BIT-SP-0004.R4 — Pages are block trees with session-stable BlockIds and byte origin spans

The core SHALL keep each open page as a tree of `Block`s addressed by session-stable `BlockId`s (never persisted). Each block loaded from disk SHALL record an `Origin` with the byte span of its own lines, its depth, a text hash and its line layout (bullet and continuation prefixes). A block SHALL be considered clean when its text hash and depth equal the origin's, regardless of sibling order or parent.

#### Scenario: Ids are stable across edits and moves
- GIVEN a page `- a\n- b\n- c\n` with ids A, B, C
- WHEN `c` is moved above `a` and `b` is edited to `bb`
- THEN the blocks are still addressed by A, B, C

#### Scenario: Moved block stays clean
- GIVEN `- a\n- b\n` loaded from disk
- WHEN `b` is moved before `a` at the same depth
- THEN both blocks are clean and serialization emits `- b\n- a\n` from the original spans

### BIT-SP-0004.R5 — Every mutation is an invertible Op inside an atomic Transaction; one user action is one undo step

The core SHALL express every mutation as primitive invertible `Op`s (`InsertSubtree`, `RemoveSubtree`, `Move`, `SetText`, `EditText`, `AdoptChildren`, `SetPreamble`, `CreatePage`, `DeletePage`, `RenameFile`) grouped into a `Transaction`. Applying a transaction SHALL be atomic: if any op fails, the applied prefix is rolled back. After every transaction the tree SHALL be acyclic, each block SHALL have exactly one parent slot, uuids SHALL be unique per graph, and `Move` SHALL never target the moved block's own descendant. User commands SHOULD be pure `plan(&Graph, Cmd) -> Result<Vec<Op>, Refusal>` functions with fixture-based tests.

#### Scenario: Inverse restores state
- GIVEN any page state S and any op `o` applied to give S'
- WHEN `o.inverse()` is applied to S'
- THEN the resulting state equals S (tree, texts, and serialized bytes)

#### Scenario: Failing op rolls back the transaction
- GIVEN a transaction `[SetText(A,"x"), Move(B into its own child)]`
- WHEN it is committed
- THEN the `Move` is refused, `SetText` is rolled back, and nothing is pushed on the undo stack

#### Scenario: Refusal explains a no-op
- GIVEN two blocks that both have children
- WHEN `MergeWithPrevious` is planned
- THEN `plan` returns `Refusal("cannot merge: both blocks have children")` and no ops

### BIT-SP-0004.R6 — Enter splits the block at the caret; Shift+Enter inserts a newline; Enter on an empty last child outdents

`Enter` in edit mode SHALL split the block: the text before the caret stays, the text after the caret (left-trimmed) goes to a new block with a new identity, which becomes the first child if the block has expanded children, otherwise the next sibling; the caret moves to the start of the new block. With the caret at 0 and text after it, an empty block SHALL be inserted before instead. `Enter` on an empty block that is the last child SHALL outdent it. `Shift+Enter` SHALL insert `\n` into the buffer without structural change. The split SHALL be one transaction.

#### Scenario: Split in the middle
- GIVEN `- Hello world` with caret after `Hello`
- WHEN the user presses `Enter`
- THEN the file contains `- Hello\n- world` and the caret is at offset 0 of `world`

#### Scenario: Block with expanded children gets a first child
- GIVEN `- parent\n\t- child` with caret at the end of `parent`
- WHEN the user presses `Enter`
- THEN the result is `- parent\n\t- \n\t- child` (new empty first child)

#### Scenario: Caret at 0 inserts before
- GIVEN `- task` with caret at 0
- WHEN `Enter` is pressed
- THEN the result is `- \n- task` and the caret stays in `task` at offset 0

#### Scenario: Empty last child outdents
- GIVEN `- a\n\t- b\n\t- ` editing the empty last child
- WHEN `Enter` is pressed
- THEN the result is `- a\n\t- b\n- `

#### Scenario: Shift+Enter
- GIVEN `- line one` with caret at end
- WHEN `Shift+Enter` then `two` is typed and committed
- THEN the file contains `- line one\n  two` (continuation indented)

### BIT-SP-0004.R7 — Backspace at offset 0 and Delete at end merge blocks with Logseq refusal rules

`Backspace` with the caret at 0 (no selection) SHALL append the block's text to the previous visible block, re-parent its children to the survivor (`AdoptChildren`) and remove it, placing the caret at the junction. It SHALL be refused when both the block and its previous sibling have children, and for the first block of a page unless that block is empty. `Delete` at the end SHALL pull the next visible block (first child or next sibling) into the current one, and SHALL be refused if that block has children. Autopair deletion (Backspace on an opening char deletes its pair) SHALL take precedence when applicable.

#### Scenario: Merge with previous
- GIVEN `- foo\n- bar` editing `bar` with caret at 0
- WHEN `Backspace` is pressed
- THEN the file becomes `- foobar` and the caret is at offset 3

#### Scenario: Refused when both have children
- GIVEN `- a\n\t- a1\n- b\n\t- b1` editing `b` at offset 0
- WHEN `Backspace` is pressed
- THEN nothing changes and a refusal notice is shown

#### Scenario: Delete at end pulls next sibling
- GIVEN `- foo\n- bar` editing `foo` with caret at 3
- WHEN `Delete` is pressed
- THEN the file becomes `- foobar`

### BIT-SP-0004.R8 — Merging away a referenced block preserves its identity

When a merge removes a block that carries an `id::` referenced elsewhere, the surviving block SHALL adopt the removed block's `id::` (Logseq behaviour, `editor.cljs:851-856`) so that `((uuid))` references and embeds keep resolving, without rewriting other files. If both blocks carry referenced uuids the merge SHALL be refused with an explanation. Undo SHALL restore both blocks with their original `id::` lines.

#### Scenario: Survivor adopts referenced uuid
- GIVEN `- foo\n- bar\n  id:: 650e…01` and another page containing `((650e…01))`
- WHEN the user merges `bar` into `foo` with Backspace
- THEN the file becomes `- foobar\n  id:: 650e…01` and the other page still renders the reference as `foobar`

#### Scenario: Both referenced
- GIVEN both `foo` and `bar` have referenced `id::` values
- WHEN Backspace at 0 in `bar`
- THEN the merge is refused and the file is unchanged

### BIT-SP-0004.R9 — Tab indents and Shift+Tab outdents blocks with direct outdenting by default

`Tab` SHALL move each top-level selected (or edited) block, with its subtree, to be the last child of its previous sibling, and SHALL expand that sibling if it was collapsed (removing `collapsed:: true`). `Shift+Tab` SHALL move the block after its parent; in direct mode (default) the block's following siblings SHALL become its children; with logical outdenting enabled they stay in place. Indent of a first child and outdent of a top-level block SHALL be no-ops. Non-consecutive selections SHALL be refused. Only leading indentation of moved clean blocks SHALL change on disk.

#### Scenario: Indent
- GIVEN `- a\n- b` editing `b`
- WHEN `Tab` is pressed
- THEN the file is `- a\n\t- b`

#### Scenario: Indent into collapsed sibling expands it
- GIVEN `- a\n  collapsed:: true\n\t- a1\n- b`
- WHEN `Tab` on `b`
- THEN the result is `- a\n\t- a1\n\t- b`

#### Scenario: Direct outdent adopts following siblings
- GIVEN `- p\n\t- x\n\t- y\n\t- z` editing `x`
- WHEN `Shift+Tab` is pressed
- THEN the result is `- p\n- x\n\t- y\n\t- z`

#### Scenario: Logical outdent
- GIVEN the same page with logical outdenting enabled
- WHEN `Shift+Tab` on `x`
- THEN the result is `- p\n\t- y\n\t- z\n- x`

### BIT-SP-0004.R10 — Alt+Shift+Up/Down moves blocks, crossing parent boundaries

`Alt+Shift+Up`/`Alt+Shift+Down` (macOS `Mod+Shift+Up/Down`) SHALL move the edited block or the top-level selected blocks, with their subtrees, one position up/down among siblings. At a sibling boundary the blocks SHALL move into the previous parent's last-child position / next parent's first-child position, as Logseq does. Moving at the very top or bottom of the page SHALL be a no-op. Clean moved blocks SHALL be emitted from their original spans.

#### Scenario: Swap with sibling
- GIVEN `- a\n- b\n- c` editing `c`
- WHEN `Alt+Shift+Up`
- THEN the file is `- a\n- c\n- b` and edit mode stays on `c`

#### Scenario: Cross parent boundary
- GIVEN `- p\n\t- x\n- q\n\t- y` editing `y`
- WHEN `Alt+Shift+Up`
- THEN `y` becomes the last child of `p`: `- p\n\t- x\n\t- y\n- q`

### BIT-SP-0004.R11 — Collapse and expand are persisted as collapsed:: true

Collapsing a block with children (click on the arrow, `Mod+Up`, or `Mod+;` toggle) SHALL hide its descendants and persist `collapsed:: true` as a property line in the block text via `EditText`, touching only that block's lines. Expanding (`Mod+Down`, arrow, toggle) SHALL remove that line. Without a target block, `Mod+Up`/`Mod+Down` SHALL collapse/expand one level of the whole page; `t o` SHALL toggle all. Blocks without children SHALL not get `collapsed::`. Loading a page SHALL honour existing `collapsed:: true`.

#### Scenario: Collapse persists one line
- GIVEN `- a\n\t- a1\n- b`
- WHEN the user collapses `a`
- THEN the file is `- a\n  collapsed:: true\n\t- a1\n- b` and only `a1` is hidden

#### Scenario: Expand removes the property
- GIVEN the collapsed page above
- WHEN `Mod+Down` is pressed while editing `a`
- THEN the file returns byte-for-byte to `- a\n\t- a1\n- b`

#### Scenario: Leaf block
- GIVEN `- leaf` without children
- WHEN `Mod+Up`
- THEN the file is unchanged

### BIT-SP-0004.R12 — Block selection with bulk delete, indent, outdent, move, copy, cut and TODO cycling

The outliner SHALL support selecting blocks: `Esc` while editing selects the edited block; `Shift+Up/Down` extends the selection in DFS order of visible blocks; `Shift+click` selects a range; `Mod+Shift+A` selects all; `Mod+A` in selection expands to the parent; `Enter` on a single selected block enters edit mode. Bulk operations (`Backspace`/`Delete`, `Tab`, `Shift+Tab`, `Alt+Shift+Up/Down`, `Mod+C`, `Mod+X`, `Mod+Enter`) SHALL act on the top-level blocks of the selection (descendants of selected blocks are implied) as a single transaction.

#### Scenario: Esc then Shift+Down
- GIVEN `- a\n- b\n- c` editing `a`
- WHEN the user presses `Esc` then `Shift+Down`
- THEN `a` and `b` are selected and none is in edit mode

#### Scenario: Delete selection is one undo step
- GIVEN `- a\n\t- a1\n- b\n- c` with `a` and `b` selected
- WHEN `Backspace` is pressed
- THEN the file is `- c` and a single `Mod+Z` restores the original bytes

#### Scenario: Bulk indent of top-level blocks only
- GIVEN `- p\n- a\n\t- a1\n- b` with `a`, `a1`, `b` selected
- WHEN `Tab`
- THEN the result is `- p\n\t- a\n\t\t- a1\n\t- b`

### BIT-SP-0004.R13 — Copy, cut and paste of block subtrees and multi-line Markdown

Copy (`Mod+C`) of a selection SHALL put three formats on the clipboard: plain Markdown (subtree re-indented from depth 0 with tabs, `id::` lines stripped), HTML, and a private MIME type carrying the subtree with uuids. Cut (`Mod+X`) SHALL copy then delete in one transaction. Pasting the private format SHALL keep uuids only after a cut and generate fresh identities (and no `id::`) after a copy. Plain-text paste matching `^\s*([-+*]|#+)\s+` SHALL be parsed into a block tree; text with blank-line-separated paragraphs SHALL become sibling blocks; anything else SHALL be inserted inline at the caret. Pasting blocks onto an empty edited block SHALL replace it. `Mod+Shift+V` SHALL paste raw text inline. Pasted CRLF SHALL be normalised to the file's line ending.

#### Scenario: Copy strips id
- GIVEN `- a\n  id:: 650e…01\n\t- b` with `a` selected
- WHEN `Mod+C`
- THEN the plain-text clipboard is `- a\n\t- b`

#### Scenario: Paste Markdown list into empty block
- GIVEN an empty edited block under `- parent`
- WHEN the clipboard text `- one\n  - two\n- three` is pasted
- THEN the empty block is replaced by `one` (with child `two`) and sibling `three`

#### Scenario: Paragraph paste
- GIVEN an empty edited block
- WHEN `first para\n\nsecond para` is pasted
- THEN two sibling blocks `first para` and `second para` are created

#### Scenario: Inline paste
- GIVEN editing `hello |` (caret at end)
- WHEN `world` is pasted
- THEN the buffer is `hello world` and no block is created

#### Scenario: Cut-paste keeps uuid
- GIVEN block `x` with `id:: 650e…02` referenced elsewhere
- WHEN `x` is cut and pasted under another block
- THEN the pasted block still has `id:: 650e…02` and the reference resolves

### BIT-SP-0004.R14 — Mod+Enter and checkbox click cycle TODO markers per preferred workflow

`Mod+Enter` (edit mode or selection) SHALL cycle the block marker by editing only the marker word: `TODO → DOING → DONE → (none) → TODO` when `:preferred-workflow` is `:todo`, and `LATER → NOW → DONE → (none) → LATER` when it is `:now` (default per Logseq config). Clicking the rendered checkbox SHALL toggle between DONE and the workflow's open marker (TODO or LATER). The rest of the block text, including priority and properties, SHALL be unchanged. On a multi-block selection, each top-level block SHALL be cycled in one transaction.

#### Scenario: TODO workflow
- GIVEN `:preferred-workflow :todo` and `- TODO call Bob`
- WHEN `Mod+Enter` is pressed three times
- THEN the block becomes `DOING call Bob`, `DONE call Bob`, then `call Bob`

#### Scenario: NOW workflow from no marker
- GIVEN `:preferred-workflow :now` and `- write spec`
- WHEN `Mod+Enter`
- THEN the block becomes `LATER write spec`

#### Scenario: Checkbox click keeps priority
- GIVEN `- TODO [#A] ship`
- WHEN the checkbox is clicked
- THEN the block becomes `DONE [#A] ship`

### BIT-SP-0004.R15 — Page autocomplete on [[ and # with autopair and on-demand page creation

Typing `[` SHALL autopair to `[]`; typing `[[` SHALL produce `[[]]` with the caret inside and open a page-search popup. `#` at line start or after whitespace SHALL open the same popup in hashtag mode. The query is the text between the trigger and the caret; results come from the index (fuzzy, excluding the current page) and include a "New page" entry when no exact match exists. Choosing an item SHALL replace the query range with `[[Page]]` or `#tag` (`#[[multi word]]` when the name has spaces) and create the page entry on demand (file created on first content). Popup keys: `Enter` complete, `Up/Down` or `Ctrl+P/N` navigate, `Esc` closes the popup only. Autopair SHALL also cover `{} () `` ~~ ** __ ^^ == ++`, skip over a typed closing char, and delete the pair on Backspace.

#### Scenario: Complete an existing page
- GIVEN the graph has page `Project Alpha` and the buffer is `see ` 
- WHEN the user types `[[proj` and presses `Enter` on `Project Alpha`
- THEN the buffer is `see [[Project Alpha]]` with the caret after `]]`

#### Scenario: Hashtag with spaces
- WHEN the user types `#road map` and chooses "New page: road map"
- THEN the buffer contains `#[[road map]]` and page `road map` exists in the index

#### Scenario: Esc closes popup only
- GIVEN the popup is open
- WHEN `Esc` is pressed
- THEN the popup closes and the block stays in edit mode

### BIT-SP-0004.R16 — Block reference autocomplete and copy-ref persist id:: on the referenced block

Typing `((` SHALL open a block-search popup (full-text, limit 20, excluding the current block and its ancestors). Choosing a block SHALL insert `((uuid))` and, in the same transaction, run `EnsureUuid` on the target: if it has no `id::`, insert `id:: <uuid v4>` right after its title line. `Mod+C` while editing with no text selection SHALL copy `((uuid))` of the current block (running `EnsureUuid`); `Mod+E` SHALL copy `{{embed ((uuid))}}`. `id::` SHALL NOT be written to blocks that are never referenced. Generated uuids SHALL be unique in the graph.

#### Scenario: Pick a block without id
- GIVEN page B contains `- Quarterly goals` with no `id::`
- WHEN on page A the user types `((quarter` and picks it
- THEN page A contains `((<u>))` and page B becomes `- Quarterly goals\n  id:: <u>`
- AND undo removes both changes in one step

#### Scenario: Block already has id
- GIVEN the target has `id:: 650e…03`
- WHEN it is picked
- THEN `((650e…03))` is inserted and page B is not modified

#### Scenario: Copy block ref
- GIVEN editing `- idea` with no text selected
- WHEN `Mod+C`
- THEN the clipboard holds `((<u>))` and the block gains `id:: <u>`

### BIT-SP-0004.R17 — Undo and redo restore exact bytes and the editor cursor

The core SHALL keep one graph-wide undo stack (capped at about 1,000 entries or 50 MB of captured text). `Mod+Z` SHALL flush the edit buffer and apply the inverses of the last transaction in reverse order, restoring `cursor_before`; `Mod+Shift+Z` / `Mod+Y` SHALL re-apply and restore `cursor_after`. Any new transaction SHALL clear the redo stack. Undo and redo SHALL go through the write path, and after undo the file on disk SHALL be byte-identical to its content before the undone transaction. External reloads SHALL NOT clear history; if an undo target no longer exists, undo SHALL stop with the notice "history truncated by external change".

#### Scenario: Byte-exact undo of a split
- GIVEN a file with bytes `- a b\r\n- c\r\n` (CRLF)
- WHEN the user splits `a b` at offset 1 and then presses `Mod+Z`
- THEN after the write the file is again exactly `- a b\r\n- c\r\n` and the caret is at offset 1 of `a b`

#### Scenario: Redo cleared by new edit
- GIVEN an undone indent
- WHEN the user types a character and commits
- THEN `Mod+Shift+Z` does nothing

#### Scenario: Target vanished
- GIVEN the last transaction edited block X and an external change deleted X
- WHEN `Mod+Z`
- THEN undo stops with the notice and the graph is unchanged

### BIT-SP-0004.R18 — Typing is coalesced into word- or pause-sized undo steps

Consecutive `EditText` ops on the same block SHOULD be merged into one undo entry while no structural op intervenes, keystrokes are less than 1.5 s apart, and no word boundary follows a pause. A structural command, a different block, or a pause ≥ 1.5 s SHALL start a new entry.

#### Scenario: Continuous typing is one step
- GIVEN an empty block
- WHEN the user types `hello` with 100 ms between keys and presses `Mod+Z`
- THEN the block is empty again

#### Scenario: Pause splits steps
- GIVEN the user typed `hello`, paused 2 s, then typed ` world`
- WHEN `Mod+Z`
- THEN the block text is `hello`

#### Scenario: Structural op breaks coalescing
- GIVEN the user typed `ab`, pressed `Tab`, typed `c`
- WHEN `Mod+Z` is pressed twice
- THEN first `c` is removed, then the indent is undone, leaving `ab`

### BIT-SP-0004.R19 — Zoom into a block with breadcrumb navigation

`Mod+.` (Win/Linux also `Alt+Right`) or a click on the bullet SHALL zoom the page view into the current block, making it the root of the view and showing a breadcrumb of its page and ancestors; `Mod+,` (`Alt+Left`) SHALL zoom out one level. Zoom is UI-only and SHALL NOT modify files. Editing operations inside a zoomed view SHALL behave as in the full page (outdenting the zoom root's children beyond the root is refused).

#### Scenario: Zoom in and out
- GIVEN `- p\n\t- c\n\t\t- g` editing `c`
- WHEN `Mod+.`
- THEN the view shows `c` as root with child `g` and breadcrumb `Page › p › c`
- AND `Mod+,` returns to showing `p` as root

#### Scenario: No file change
- WHEN the user zooms in and out
- THEN the page file mtime and bytes are unchanged

### BIT-SP-0004.R20 — IME composition is supported in the block editor

The `BlockEditor` SHALL implement GPUI `EntityInputHandler` (marked text, `replace_text_in_range`, `replace_and_mark_text_in_range`, `bounds_for_range`) so that IMEs for CJK, dead keys and emoji pickers work on Linux (IBus/Fcitx), macOS and Windows. The candidate window SHALL be positioned at the caret. Key bindings (`Enter`, `Backspace`, `Tab`, arrows) SHALL NOT trigger outliner commands while a composition is active, and the buffer SHALL NOT be committed mid-composition.

#### Scenario: Japanese composition with Enter
- GIVEN an empty edited block and a Japanese IME
- WHEN the user types `nihongo` and presses `Enter` to confirm the candidate `日本語`
- THEN the buffer is `日本語` and no block split happens
- AND a second `Enter` splits the block

#### Scenario: Dead key
- GIVEN a US-International layout
- WHEN the user types `'` then `e`
- THEN the buffer contains `é`

### BIT-SP-0004.R21 — Logseq default keymap in four key contexts with overridable bindings

The app SHOULD expose Logseq's default editor bindings (see [[04-editor-outliner-operations]] §7) through four GPUI key contexts with precedence `Autocomplete` > `BlockEditor` > `BlockSelection` > `Outliner`, and SHOULD allow overriding bindings through a keymap file in the app config directory. `Mod` SHALL map to Cmd on macOS and Ctrl elsewhere; platform-specific defaults (e.g. move up/down `Mod+Shift+Up` on macOS vs `Alt+Shift+Up` elsewhere) SHALL follow Logseq.

#### Scenario: Context precedence
- GIVEN the page-search popup is open while editing
- WHEN `Enter` is pressed
- THEN the popup item is chosen and the block is not split

#### Scenario: Override binding
- GIVEN the keymap file binds `editor::CycleTodo` to `ctrl-t` in context `BlockEditor`
- WHEN the user presses `Ctrl+T` while editing `- task`
- THEN the block becomes `TODO task` (or `LATER task` in the NOW workflow)

#### Scenario: Invalid keymap entry
- GIVEN the keymap file contains an unknown action name
- WHEN the app starts
- THEN defaults remain active and a warning lists the invalid entry
