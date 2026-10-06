//! Keystroke-level `#[gpui::test]` suite of the spike editor (BIT-US-0040).

use super::doc::SpikeDoc;
use super::editor::{Caret, MOD, SpikeEditor, bind_keys};
use super::inline::InlineRender;
use crate::settings::AppSettings;
use crate::theme;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::ui::text_edit::{EntityInputHandler as _, Modifiers};
use crate::ui::{Bounds, Entity, point, px, size};

fn doc(items: &[(u8, &str)]) -> SpikeDoc {
    SpikeDoc::from_blocks(items.iter().map(|(d, t)| (*d, (*t).to_owned())))
}

fn open(
    cx: &mut TestAppContext,
    doc: SpikeDoc,
    focus: usize,
) -> (Entity<SpikeEditor>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
        bind_keys(cx);
    });
    cx.add_window_view(|window, cx| {
        let mut editor = SpikeEditor::new(doc, false, cx);
        editor.focus_block(focus, Caret::End, window, cx);
        editor
    })
}

fn texts(ed: &Entity<SpikeEditor>, cx: &mut VisualTestContext) -> Vec<(u8, String)> {
    ed.read_with(cx, |e, _| {
        e.doc()
            .blocks
            .iter()
            .map(|b| (b.depth, b.text.clone()))
            .collect()
    })
}

fn at(ed: &Entity<SpikeEditor>, cx: &mut VisualTestContext) -> (Option<usize>, usize) {
    ed.read_with(cx, |e, _| (e.focused(), e.cursor_offset()))
}

fn pair(depth: u8, text: &str) -> (u8, String) {
    (depth, text.to_owned())
}

#[gpui_test]
fn typing_goes_through_the_input_handler(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "")]), 0);
    cx.simulate_input("h\u{e9}llo");
    assert_eq!(texts(&ed, cx), [pair(0, "h\u{e9}llo")]);
    assert_eq!(at(&ed, cx), (Some(0), 6));
}

#[gpui_test]
fn enter_splits_and_backspace_at_start_merges(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "abcd")]), 0);
    cx.simulate_keystrokes("left left enter");
    assert_eq!(texts(&ed, cx), [pair(0, "ab"), pair(0, "cd")]);
    assert_eq!(at(&ed, cx), (Some(1), 0));
    assert_eq!(ed.read_with(cx, |e, _| e.rows().to_vec()), [0, 1]);
    cx.simulate_keystrokes("backspace");
    assert_eq!(texts(&ed, cx), [pair(0, "abcd")]);
    assert_eq!(at(&ed, cx), (Some(0), 2));
    assert_eq!(ed.read_with(cx, |e, _| e.list_state().item_count()), 1);
}

#[gpui_test]
fn enter_on_an_empty_nested_block_outdents_it(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "a"), (1, "")]), 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(texts(&ed, cx), [pair(0, "a"), pair(0, "")]);
}

#[gpui_test]
fn delete_at_end_pulls_up_the_next_block(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "ab"), (0, "cd")]), 0);
    cx.simulate_keystrokes("delete");
    assert_eq!(texts(&ed, cx), [pair(0, "abcd")]);
    assert_eq!(at(&ed, cx), (Some(0), 2));
}

#[gpui_test]
fn tab_and_shift_tab_change_depth(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "a"), (0, "b")]), 0);
    cx.simulate_keystrokes("tab");
    assert_eq!(texts(&ed, cx)[0].0, 0, "first block cannot be indented");
    cx.simulate_keystrokes("down tab");
    assert_eq!(texts(&ed, cx)[1].0, 1);
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(texts(&ed, cx)[1].0, 0);
}

#[gpui_test]
fn vertical_navigation_keeps_goal_x_across_blocks(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "abcdef"), (0, "xy"), (0, "0123456789")]), 0);
    cx.simulate_keystrokes("home right right right right");
    assert_eq!(at(&ed, cx), (Some(0), 4));
    cx.simulate_keystrokes("down");
    assert_eq!(at(&ed, cx), (Some(1), 2), "clamped to the short block");
    cx.simulate_keystrokes("down");
    assert_eq!(at(&ed, cx), (Some(2), 4), "goal x is remembered");
    cx.simulate_keystrokes("up up");
    assert_eq!(at(&ed, cx), (Some(0), 4));
}

#[gpui_test]
fn left_and_right_cross_block_edges(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "ab"), (0, "cd")]), 0);
    cx.simulate_keystrokes("right");
    assert_eq!(at(&ed, cx), (Some(1), 0));
    cx.simulate_keystrokes("left");
    assert_eq!(at(&ed, cx), (Some(0), 2));
}

#[gpui_test]
fn soft_wrapped_rows_are_visited_before_leaving_the_block(cx: &mut TestAppContext) {
    let long = "word ".repeat(120);
    let (ed, cx) = open(cx, doc(&[(0, long.trim_end()), (0, "next")]), 0);
    cx.simulate_resize(size(px(360.), px(500.)));
    ed.update_in(cx, |e, window, cx| {
        e.focus_block(0, Caret::Start, window, cx)
    });
    let rows = ed.update_in(cx, |e, window, cx| {
        e.shape_focused(window, cx).map_or(0, |l| l.row_count())
    });
    assert!(rows > 3, "expected soft wrap, got {rows} rows");
    for step in 1..rows {
        cx.simulate_keystrokes("down");
        let (block, cursor) = at(&ed, cx);
        assert_eq!(block, Some(0), "left the block early at step {step}");
        assert!(cursor > 0);
    }
    cx.simulate_keystrokes("down");
    assert_eq!(at(&ed, cx).0, Some(1));
    // And back up into the last visual row of the long block.
    cx.simulate_keystrokes("up");
    assert_eq!(at(&ed, cx).0, Some(0));
    let (cursor, len) = ed.read_with(cx, |e, _| (e.cursor_offset(), e.focused_text().len()));
    assert!(cursor > len / 2, "caret should land on the last rows");
}

#[gpui_test]
fn shift_arrows_select_and_typing_replaces(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "hello")]), 0);
    cx.simulate_keystrokes("shift-left shift-left");
    assert_eq!(ed.read_with(cx, |e, _| e.selection_range()), 3..5);
    cx.simulate_input("p");
    assert_eq!(texts(&ed, cx), [pair(0, "help")]);
}

#[gpui_test]
fn ime_marked_text_round_trip_through_the_input_handler(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "ab")]), 0);
    cx.simulate_keystrokes("left");
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "ni hao", Some(2..2), window, cx);
    });
    assert_eq!(texts(&ed, cx), [pair(0, "ani haob")]);
    let (marked, selection) = ed.update_in(cx, |e, window, cx| {
        (
            e.marked_text_range(window, cx),
            e.selected_text_range(false, window, cx),
        )
    });
    assert_eq!(marked, Some(1..7));
    assert_eq!(selection.map(|s| s.range), Some(3..3));
    // A later preedit update replaces the marked range only.
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "\u{4f60}\u{597d}", None, window, cx);
    });
    assert_eq!(texts(&ed, cx), [pair(0, "a\u{4f60}\u{597d}b")]);
    let marked = ed.update_in(cx, |e, window, cx| e.marked_text_range(window, cx));
    assert_eq!(marked, Some(1..3));
    // Commit.
    ed.update_in(cx, |e, window, cx| {
        e.replace_text_in_range(None, "\u{4f60}\u{597d}", window, cx);
    });
    assert_eq!(texts(&ed, cx), [pair(0, "a\u{4f60}\u{597d}b")]);
    assert_eq!(ed.read_with(cx, |e, _| e.marked_range()), None);
    assert_eq!(at(&ed, cx).1, 7);
    // UTF-16 text access.
    let slice = ed.update_in(cx, |e, window, cx| {
        let mut actual = None;
        let text = e.text_for_range(1..3, &mut actual, window, cx);
        (text, actual)
    });
    assert_eq!(slice, (Some("\u{4f60}\u{597d}".to_owned()), Some(1..3)));
}

#[gpui_test]
fn ime_enter_commits_composition_instead_of_splitting(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "")]), 0);
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "kan", None, window, cx);
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(texts(&ed, cx), [pair(0, "kan")]);
    assert_eq!(ed.read_with(cx, |e, _| e.marked_range()), None);
}

#[gpui_test]
fn ime_bounds_follow_the_caret(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "abcdef")]), 0);
    let element = Bounds::new(point(px(10.), px(20.)), size(px(300.), px(22.)));
    let (at0, at4) = ed.update_in(cx, |e, window, cx| {
        (
            e.bounds_for_range(0..0, element, window, cx),
            e.bounds_for_range(4..4, element, window, cx),
        )
    });
    let (at0, at4) = (at0.expect("bounds at 0"), at4.expect("bounds at 4"));
    assert_eq!(at0.origin, element.origin);
    assert!(at4.origin.x > at0.origin.x);
    assert_eq!(at4.origin.y, at0.origin.y);
    assert!(at0.size.height > px(0.));
    let idx = ed.update_in(cx, |e, window, cx| {
        e.character_index_for_point(point(px(0.), px(0.)), window, cx)
    });
    assert!(idx.is_some());
}

#[gpui_test]
fn clicking_an_unfocused_block_focuses_it_with_a_source_offset(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "a **bold** z"), (0, "other")]), 1);
    // Mapping contract: display offset 4 ("a bo|ld z") is source offset 6.
    ed.update_in(cx, |e, window, cx| {
        let render = InlineRender::new("a **bold** z");
        e.click_unfocused(0, 4, &render, window, cx);
    });
    assert_eq!(at(&ed, cx), (Some(0), 6));
    // A real mouse event on the other row focuses it.
    let bounds = ed
        .read_with(cx, |e, _| e.list_state().bounds_for_item(1))
        .expect("row 1 laid out");
    cx.simulate_click(
        point(bounds.right() - px(20.), bounds.top() + px(10.)),
        Modifiers::default(),
    );
    assert_eq!(ed.read_with(cx, |e, _| e.focused()), Some(1));
    assert_eq!(at(&ed, cx).1, "other".len(), "click right of the text: end");
}

#[gpui_test]
fn collapse_and_expand_splice_only_the_subtree(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "a"), (1, "a1"), (1, "a2"), (0, "b")]), 0);
    assert_eq!(ed.read_with(cx, |e, _| e.list_state().item_count()), 4);
    cx.simulate_keystrokes(&format!("{MOD}-up"));
    assert_eq!(ed.read_with(cx, |e, _| e.rows().to_vec()), [0, 3]);
    assert_eq!(ed.read_with(cx, |e, _| e.list_state().item_count()), 2);
    cx.simulate_keystrokes(&format!("{MOD}-down"));
    assert_eq!(ed.read_with(cx, |e, _| e.rows().to_vec()), [0, 1, 2, 3]);
    assert_eq!(ed.read_with(cx, |e, _| e.list_state().item_count()), 4);
}

#[gpui_test]
fn clipboard_copy_cut_paste(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "hello world")]), 0);
    cx.simulate_keystrokes(&format!("{MOD}-a {MOD}-c right {MOD}-v"));
    assert_eq!(texts(&ed, cx), [pair(0, "hello worldhello world")]);
    cx.simulate_keystrokes(&format!("{MOD}-a {MOD}-x"));
    assert_eq!(texts(&ed, cx), [pair(0, "")]);
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()),
        Some("hello worldhello world".to_owned())
    );
}

#[gpui_test]
fn in_block_undo_and_redo(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "")]), 0);
    cx.simulate_input("abc de");
    assert_eq!(texts(&ed, cx), [pair(0, "abc de")]);
    cx.simulate_keystrokes(&format!("{MOD}-z"));
    assert_eq!(texts(&ed, cx), [pair(0, "abc")]);
    cx.simulate_keystrokes(&format!("{MOD}-z"));
    assert_eq!(texts(&ed, cx), [pair(0, "")]);
    cx.simulate_keystrokes(&format!("{MOD}-shift-z"));
    assert_eq!(texts(&ed, cx), [pair(0, "abc")]);
}

#[gpui_test]
fn shift_enter_inserts_a_soft_newline_and_the_block_gets_two_rows(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "ab")]), 0);
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("c");
    assert_eq!(texts(&ed, cx), [pair(0, "ab\nc")]);
    let rows = ed.update_in(cx, |e, window, cx| {
        e.shape_focused(window, cx).map_or(0, |l| l.row_count())
    });
    assert_eq!(rows, 2);
    cx.simulate_keystrokes("up");
    assert_eq!(at(&ed, cx), (Some(0), 1), "up stays inside the block");
}

#[gpui_test]
fn layout_hit_testing_and_selection_rects(cx: &mut TestAppContext) {
    let (ed, cx) = open(cx, doc(&[(0, "abcdef ghij")]), 0);
    let layout = ed.update_in(cx, |e, window, cx| {
        e.shape_focused(window, cx).expect("focused")
    });
    let p3 = layout.position_for_index(3);
    let p0 = layout.position_for_index(0);
    assert!(p3.x > p0.x);
    // Round trip: the caret position hits the same offset.
    assert_eq!(layout.index_for_position(p3), 3);
    let rects = layout.selection_rects(2..5);
    assert_eq!(rects.len(), 1);
    assert!(rects[0].size.width > px(0.));
    assert_eq!(rects[0].origin.x, layout.position_for_index(2).x);
    assert!(layout.selection_rects(4..4).is_empty());
}
