//! `#[gpui::test]` suite of the `SCHEDULED` / `DEADLINE` date chips: open the picker from the
//! chip, pick or remove a day, and check the file bytes and the undo step (BIT-US-0167).

use std::rc::Rc;

use bitacora_core::date::Date;

use super::*;
use crate::views::planning::{DEADLINE, SCHEDULED};

const SRC: &str = "- TODO task\n  SCHEDULED: <2025-11-20 Thu 09:30 .+1d>\n  DEADLINE: <2025-12-01 Mon>\n  note:: keep\n- other\n";

fn set_clock(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) {
    ed.update(cx, |e, _| {
        e.set_clock(Rc::new(|| {
            Some((Date::new(2025, 11, 14).expect("date"), "09:05".to_owned()))
        }));
    });
}

fn hooks(
    ed: &Entity<OutlineEditor>,
    cx: &mut VisualTestContext,
    row: usize,
) -> crate::views::planning::PlanningActions {
    cx.update(|_, cx| OutlineEditor::row_edit(ed, row, cx))
        .and_then(|h| h.planning)
        .expect("planning hooks")
}

#[gpui_test]
fn picking_a_day_rewrites_only_the_date_and_keeps_the_repeater(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, SRC)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    let h = hooks(&ed, cx, 0);
    cx.update(|window, cx| (h.on_open)(SCHEDULED, window, cx));
    let open = ed.read_with(cx, |e, _| e.planning_open()).expect("open");
    assert_eq!(open.1.keyword, SCHEDULED);
    assert_eq!((open.1.month.year, open.1.month.month), (2025, 11));
    // The month arrows move the displayed month.
    ed.update(cx, |e, cx| e.shift_planning(1, cx));
    assert_eq!(
        ed.read_with(cx, |e, _| e.planning_open().map(|o| o.1.month.month)),
        Some(12)
    );
    ed.update_in(cx, |e, window, cx| e.pick_planning(20_251_203, window, cx));
    assert!(ed.read_with(cx, |e, _| e.planning_open()).is_none());
    assert_eq!(
        env.disk(HOME),
        "- TODO task\n  SCHEDULED: <2025-12-03 Wed 09:30 .+1d>\n  DEADLINE: <2025-12-01 Mon>\n  note:: keep\n- other\n"
    );
    // One undo step restores the exact bytes.
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.disk(HOME), SRC);
}

#[gpui_test]
fn removing_a_deadline_drops_its_line_and_is_undoable(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, SRC)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    let h = hooks(&ed, cx, 0);
    cx.update(|window, cx| (h.on_open)(DEADLINE, window, cx));
    ed.update_in(cx, |e, window, cx| e.clear_planning_date(window, cx));
    assert_eq!(
        env.disk(HOME),
        "- TODO task\n  SCHEDULED: <2025-11-20 Thu 09:30 .+1d>\n  note:: keep\n- other\n"
    );
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.disk(HOME), SRC);
}

#[gpui_test]
fn picking_while_the_block_is_edited_keeps_what_was_typed(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, SRC)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    let h = hooks(&ed, cx, 0);
    cx.update(|window, cx| (h.on_open)(SCHEDULED, window, cx));
    ed.update_in(cx, |e, window, cx| e.pick_planning(20_251_121, window, cx));
    let disk = env.disk(HOME);
    // The caret was at the end of the block: the typed "!" and the new date are both kept.
    assert_eq!(
        disk,
        "- TODO task\n  SCHEDULED: <2025-11-21 Fri 09:30 .+1d>\n  DEADLINE: <2025-12-01 Mon>\n  note:: keep!\n- other\n"
    );
}

#[gpui_test]
fn slash_scheduled_replaces_the_line_under_the_title_and_undo_restores_it(cx: &mut TestAppContext) {
    let env = Env::new(&[(
        HOME,
        "- TODO task\n  SCHEDULED: <2025-11-20 Thu>\n  note:: keep\n",
    )]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    edit(&ed, 0, Caret::Visible(9), cx);
    cx.simulate_input(" /scheduled");
    cx.simulate_keystrokes("enter");
    assert!(ed.read_with(cx, |e, _| e.picker_state().is_some()));
    cx.simulate_keystrokes("right enter");
    flush(&ed, cx);
    // Logseq's picker writes a plain timestamp over the old one (no second line).
    assert_eq!(
        env.disk(HOME),
        "- TODO task\n  SCHEDULED: <2025-11-15 Sat>\n  note:: keep\n"
    );
    cx.simulate_keystrokes(&k("ctrl-z"));
    flush(&ed, cx);
    assert_eq!(
        env.disk(HOME),
        "- TODO task\n  SCHEDULED: <2025-11-20 Thu>\n  note:: keep\n"
    );
}

#[gpui_test]
fn clicking_the_rendered_chip_opens_the_picker(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, SRC)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("planning-chip-0-0")
        .expect("the chip is drawn");
    cx.simulate_click(bounds.center(), Default::default());
    assert!(ed.read_with(cx, |e, _| e.planning_open()).is_some());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("planning-picker-0-0").is_some(),
        "the popover is drawn next to the chip"
    );
}
