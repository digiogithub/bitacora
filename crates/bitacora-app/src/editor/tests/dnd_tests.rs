//! `#[gpui::test]` suite of block drag and drop (BIT-US-0106): real mouse presses on bullets,
//! the drop zones, multi-block drags, Alt-drop references, never dropping into a block's own
//! subtree and the single undo step.

use super::*;
use crate::editor::dnd::DropZone;
use crate::ui::text_edit::{Modifiers, MouseButton};
use crate::ui::{Bounds, Pixels, Point, point};

fn bullet(b: Bounds<Pixels>) -> Point<Pixels> {
    // Row padding 8 + fold arrow 14 + gap 4 + half the bullet slot.
    point(b.left() + px(32.), b.top() + px(12.))
}

fn lower_part(b: Bounds<Pixels>) -> Point<Pixels> {
    point(b.left() + px(50.), b.bottom() - px(4.))
}

fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>, mods: Modifiers) {
    cx.simulate_mouse_down(from, MouseButton::Left, mods);
    // Several moves: the drag starts after the pointer left the press position.
    cx.simulate_mouse_move(
        point(from.x + px(6.), from.y + px(6.)),
        MouseButton::Left,
        mods,
    );
    cx.simulate_mouse_move(
        point(from.x + px(8.), from.y + px(12.)),
        MouseButton::Left,
        mods,
    );
    cx.simulate_mouse_move(to, MouseButton::Left, mods);
    cx.simulate_mouse_move(to, MouseButton::Left, mods);
    cx.simulate_mouse_up(to, MouseButton::Left, mods);
    cx.run_until_parked();
}

fn texts(env: &Env, title: &str) -> Vec<(usize, String)> {
    env.snapshot_texts(title)
}

#[gpui_test]
fn dragging_a_bullet_below_another_block_moves_it_in_one_undo_step(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n- three\n- four\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let first = row_bounds(&view, 0, cx);
    let third = row_bounds(&view, 2, cx);
    drag(cx, bullet(first), lower_part(third), Modifiers::default());
    assert_eq!(
        texts(&env, "Home"),
        [
            pair(1, "two"),
            pair(1, "three"),
            pair(1, "one"),
            pair(1, "four")
        ]
    );
    // The moved block stays selected and nothing was left in edit mode.
    let moved = ids(&ed, cx)[2];
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks()), [moved]);
    assert_eq!(env.disk(HOME), "- two\n- three\n- one\n- four\n");
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(
        texts(&env, "Home"),
        [
            pair(1, "one"),
            pair(1, "two"),
            pair(1, "three"),
            pair(1, "four")
        ]
    );
}

#[gpui_test]
fn the_pointer_x_and_y_choose_before_after_or_child(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n- three\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let one = row_bounds(&view, 0, cx);
    let three = row_bounds(&view, 2, cx);
    // Upper part of the row: before.
    drag(
        cx,
        bullet(three),
        point(one.left() + px(70.), one.top() + px(3.)),
        Modifiers::default(),
    );
    assert_eq!(
        texts(&env, "Home"),
        [pair(1, "three"), pair(1, "one"), pair(1, "two")]
    );
    // Lower part, far right of the text: as a child.
    let one = row_bounds(&view, 1, cx);
    let two = row_bounds(&view, 2, cx);
    let _ = two;
    let zero = row_bounds(&view, 0, cx);
    drag(
        cx,
        bullet(zero),
        point(one.left() + px(200.), one.bottom() - px(4.)),
        Modifiers::default(),
    );
    assert_eq!(
        texts(&env, "Home"),
        [pair(1, "one"), pair(2, "three"), pair(1, "two")]
    );
    // While dragging, the editor shows where the block would land.
    let _ = ed;
}

#[gpui_test]
fn the_indicator_follows_the_pointer_during_the_drag(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n- three\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let one = row_bounds(&view, 0, cx);
    let three = row_bounds(&view, 2, cx);
    let from = bullet(one);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(from.x + px(8.), from.y + px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(lower_part(three), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(lower_part(three), MouseButton::Left, Modifiers::default());
    assert_eq!(
        ed.read_with(cx, |e, _| e.drop_zone(2)),
        Some(DropZone::After)
    );
    assert_eq!(ed.read_with(cx, |e, _| e.drop_zone(0)), None);
    cx.simulate_mouse_move(
        point(three.left() + px(70.), three.top() + px(2.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert_eq!(
        ed.read_with(cx, |e, _| e.drop_zone(2)),
        Some(DropZone::Before)
    );
    cx.simulate_mouse_up(
        point(three.left() + px(70.), three.top() + px(2.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        ed.read_with(cx, |e, _| e.drop_zone(2)),
        None,
        "indicator gone"
    );
    assert_eq!(
        texts(&env, "Home"),
        [pair(1, "two"), pair(1, "one"), pair(1, "three")]
    );
}

#[gpui_test]
fn dragging_a_selected_block_moves_the_whole_selection_in_order(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n- three\n- four\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let all = ids(&ed, cx);
    ed.update_in(cx, |e, window, cx| e.select_only(all[0], window, cx));
    cx.simulate_keystrokes("shift-down");
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks().len()), 2);
    let two = row_bounds(&view, 1, cx);
    let four = row_bounds(&view, 3, cx);
    drag(cx, bullet(two), lower_part(four), Modifiers::default());
    assert_eq!(
        texts(&env, "Home"),
        [
            pair(1, "three"),
            pair(1, "four"),
            pair(1, "one"),
            pair(1, "two")
        ]
    );
    assert_eq!(
        ed.read_with(cx, |e, _| e.selected_blocks().len()),
        2,
        "both moved blocks stay selected"
    );
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(texts(&env, "Home")[0], pair(1, "one"));
}

#[gpui_test]
fn a_block_is_never_dropped_into_its_own_subtree(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- parent\n\t- child\n- other\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let parent = row_bounds(&view, 0, cx);
    let child = row_bounds(&view, 1, cx);
    let before = env.disk(HOME);
    let from = bullet(parent);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(from.x + px(8.), from.y + px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(lower_part(child), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(lower_part(child), MouseButton::Left, Modifiers::default());
    assert_eq!(
        ed.read_with(cx, |e, _| e.drop_zone(1)),
        None,
        "no indicator inside the dragged subtree"
    );
    cx.simulate_mouse_up(lower_part(child), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    assert_eq!(env.disk(HOME), before);
    assert_eq!(
        texts(&env, "Home"),
        [pair(1, "parent"), pair(2, "child"), pair(1, "other")]
    );
}

#[gpui_test]
fn alt_drop_inserts_a_reference_and_keeps_the_source(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- source\n- other\n")]);
    let (view, _ed, cx) = open_page(cx, &env, "Home");
    let source = row_bounds(&view, 0, cx);
    let other = row_bounds(&view, 1, cx);
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    drag(cx, bullet(source), lower_part(other), alt);
    let t = texts(&env, "Home");
    assert_eq!(t.len(), 3, "{t:?}");
    assert!(t[0].1.starts_with("source"), "{t:?}");
    assert_eq!(t[1].1, "other");
    assert!(t[2].1.starts_with("(("), "{t:?}");
    let disk = env.disk(HOME);
    let uuid = t[2]
        .1
        .trim_start_matches("((")
        .trim_end_matches("))")
        .to_owned();
    assert!(
        disk.contains(&format!("- source\n  id:: {uuid}\n")),
        "{disk}"
    );
    // One undo removes the reference and the id::.
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.disk(HOME), "- source\n- other\n");
}

#[gpui_test]
fn dropping_on_another_day_moves_across_pages_in_one_transaction(cx: &mut TestAppContext) {
    use crate::views::journals::JournalsView;
    use bitacora_core::date::Date;
    let env = Env::new(&[
        ("journals/2025_03_09.md", "- standup\n- notes\n"),
        ("journals/2025_03_08.md", "- yesterday\n"),
    ]);
    setup(cx);
    let day = Date::new(2025, 3, 9).expect("date");
    let (feed, cx) =
        cx.add_window_view(|_, _| JournalsView::with_clock(std::rc::Rc::new(move || Some(day))));
    cx.simulate_resize(size(px(900.), px(700.)));
    let link = env.link.clone();
    feed.update(cx, |v, cx| v.set_session_link(Some(link), cx));
    let handle = env.handle.clone();
    feed.update(cx, |v, cx| v.show(handle, None, cx));
    cx.executor().allow_parking();
    let (d9, d8) = (day.journal_day(), 20_250_308);
    for _ in 0..400 {
        cx.run_until_parked();
        if feed.read_with(cx, |v, _| {
            v.editor_for(d9).is_some() && v.editor_for(d8).is_some()
        }) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let top = feed
        .read_with(cx, |v, _| v.editor_for(d9).cloned())
        .expect("today is editable");
    let older = feed
        .read_with(cx, |v, _| v.editor_for(d8).cloned())
        .expect("older day is editable");
    let moved = ids(&top, cx)[0];
    let target = ids(&older, cx)[0];
    let drag_payload = top.read_with(cx, |e, _| e.drag_payload(moved));
    older.update_in(cx, |e, window, cx| {
        // The pointer is over the lower part of the other day's only block.
        e.drop_on(0, &drag_payload, false, window, cx);
    });
    let _ = target;
    assert_eq!(texts(&env, "Mar 9th, 2025"), [pair(1, "notes")]);
    assert_eq!(
        texts(&env, "Mar 8th, 2025"),
        [pair(1, "yesterday"), pair(1, "standup")]
    );
    // The source day's editor already shows the result.
    assert_eq!(top.read_with(cx, |e, _| e.rows().len()), 1);
    assert_eq!(env.disk("journals/2025_03_09.md"), "- notes\n");
    assert_eq!(
        env.disk("journals/2025_03_08.md"),
        "- yesterday\n- standup\n"
    );
}
