//! `#[gpui::test]` suite of the slash menu, the angle-bracket commands, templates and the
//! calendar (BIT-US-0105). Same setup as the parent suite: a real graph, a live session and
//! simulated keystrokes.

use std::rc::Rc;

use bitacora_core::date::Date;

use super::*;
use crate::editor::completion::Item;

const TPL_PAGE: &str = "pages/Templates.md";
const TPL: &str = "- Meeting notes\n  template:: meeting\n  - Date: <% today %> at <% time %>\n  - Page <% current page %>\n    - tomorrow <% tomorrow %>\n- Standup\n  template:: standup\n";

fn fixed_day() -> Date {
    Date::new(2025, 11, 14).expect("date")
}

fn set_clock(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) {
    ed.update(cx, |e, _| {
        e.set_clock(Rc::new(|| Some((fixed_day(), "09:05".to_owned()))));
    });
}

fn labels(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) -> Vec<String> {
    ed.read_with(cx, |e, _| {
        e.completion()
            .map(|c| c.items.iter().map(Item::label).collect())
            .unwrap_or_default()
    })
}

#[gpui_test]
fn the_slash_menu_lists_the_catalogue_filters_and_sets_a_marker(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /");
    let all = labels(&ed, cx);
    for want in [
        "TODO",
        "DOING",
        "LATER",
        "NOW",
        "Heading 1",
        "Page reference",
        "Block reference",
        "Page embed",
        "Block embed",
        "Query",
        "Today",
        "Tomorrow",
        "Yesterday",
        "Date picker",
        "Scheduled",
        "Deadline",
        "Template",
        "Code block",
        "Link",
        "Image link",
        "Upload an asset",
    ] {
        assert!(all.iter().any(|l| l == want), "{want} in {all:?}");
    }
    cx.simulate_input("doi");
    assert_eq!(labels(&ed, cx)[0], "DOING");
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("DOING one"));
    assert!(!ed.read_with(cx, |e, _| e.completion_open()));
    flush(&ed, cx);
    assert_eq!(env.disk(HOME), "- DOING one\n- two\n");
}

#[gpui_test]
fn the_slash_menu_navigates_with_the_keyboard_and_closes_with_escape(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /");
    cx.simulate_keystrokes("down down");
    assert_eq!(
        ed.read_with(cx, |e, _| e.completion().map(|c| c.selected)),
        Some(2)
    );
    cx.simulate_keystrokes("up");
    // Wrapping from the first to the last entry scrolls the window.
    cx.simulate_keystrokes("up up");
    let last = labels(&ed, cx).len() - 1;
    assert_eq!(
        ed.read_with(cx, |e, _| e.completion().map(|c| c.selected)),
        Some(last)
    );
    cx.simulate_keystrokes("escape");
    assert!(!ed.read_with(cx, |e, _| e.completion_open()));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one /"));
    assert_eq!(editing_row(&ed, cx), Some(0), "still editing");
}

#[gpui_test]
fn a_slash_that_matches_nothing_keeps_enter_for_the_block(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" and/or /zzzz");
    assert!(!ed.read_with(cx, |e, _| e.completion_open()));
    cx.simulate_keystrokes("enter");
    assert_eq!(env.snapshot_texts("Home").len(), 2);
}

#[gpui_test]
fn headings_refs_embeds_and_query_insert_logseq_text(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /h2");
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("## one"));
    cx.simulate_input(" /page embed");
    assert_eq!(labels(&ed, cx)[0], "Page embed");
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("## one {{embed [[]]}}"));
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        "## one {{embed [[".len()
    );
    cx.simulate_keystrokes("end");
    cx.simulate_input(" /query");
    cx.simulate_keystrokes("enter");
    assert!(
        buffer(&ed, cx)
            .as_deref()
            .is_some_and(|t| t.ends_with(" {{query }}")),
        "{:?}",
        buffer(&ed, cx)
    );
    flush(&ed, cx);
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one"), "one undo step");
}

#[gpui_test]
fn date_commands_use_the_journal_title(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- on\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /today");
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("on [[Nov 14th, 2025]]"));
    cx.simulate_input(" /tomorrow");
    cx.simulate_keystrokes("enter");
    cx.simulate_input(" /yesterday");
    cx.simulate_keystrokes("enter");
    cx.simulate_input(" /current time");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("on [[Nov 14th, 2025]] [[Nov 15th, 2025]] [[Nov 13th, 2025]] 09:05")
    );
}

#[gpui_test]
fn angle_commands_write_begin_end_blocks(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("<");
    let all = labels(&ed, cx);
    for want in [
        "Quote",
        "Source code",
        "Note",
        "Tip",
        "Important",
        "Caution",
        "Warning",
        "Example",
        "Export",
        "Center",
        "Comment",
        "Verse",
        "Query",
    ] {
        assert!(all.iter().any(|l| l == want), "{want} in {all:?}");
    }
    cx.simulate_input("quo");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("one\n#+BEGIN_QUOTE\n\n#+END_QUOTE")
    );
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        "one\n#+BEGIN_QUOTE\n".len()
    );
    cx.simulate_input("quoted");
    flush(&ed, cx);
    assert_eq!(
        env.disk(HOME),
        "- one\n  #+BEGIN_QUOTE\n  quoted\n  #+END_QUOTE\n"
    );
}

#[gpui_test]
fn the_template_list_inserts_expanded_blocks_in_one_undo_step(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- x\n"), (TPL_PAGE, TPL)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("/template mee");
    let all = labels(&ed, cx);
    assert_eq!(all, ["meeting"], "only the matching template");
    cx.simulate_keystrokes("enter");
    flush(&ed, cx);
    let texts = env.snapshot_texts("Home");
    assert_eq!(texts[0].1, "one");
    assert_eq!(texts[1].1, "Date: [[Nov 14th, 2025]] at 09:05");
    assert_eq!(texts[2].1, "Page [[Home]]");
    assert_eq!(texts[3], (2, "tomorrow [[Nov 15th, 2025]]".to_owned()));
    assert_eq!(texts.len(), 4, "the trigger block was replaced");
    // The template page is untouched.
    assert_eq!(env.disk(TPL_PAGE), TPL);
    // One undo brings the trigger block back.
    cx.simulate_keystrokes(&k("ctrl-z"));
    let texts = env.snapshot_texts("Home");
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[1].1, "/template mee");
}

#[gpui_test]
fn choosing_template_in_the_slash_menu_switches_to_the_template_list(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n"), (TPL_PAGE, TPL)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /templ");
    assert_eq!(labels(&ed, cx)[0], "Template");
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one /template "));
    assert_eq!(labels(&ed, cx), ["meeting", "standup"]);
    cx.simulate_keystrokes("down enter");
    // `standup` is a template block without children: the block itself is the template.
    flush(&ed, cx);
    let texts = env.snapshot_texts("Home");
    assert_eq!(texts[0].1, "one");
    assert_eq!(texts[1].1, "Standup");
}

#[gpui_test]
fn a_graph_without_templates_says_so_and_leaves_the_text(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /templ");
    cx.simulate_keystrokes("enter");
    assert!(!ed.read_with(cx, |e, _| e.completion_open()));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one"));
}

#[gpui_test]
fn the_calendar_writes_scheduled_deadline_and_date_links(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- TODO task\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    set_clock(&ed, cx);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /scheduled");
    cx.simulate_keystrokes("enter");
    assert!(ed.read_with(cx, |e, _| e.picker_state().is_some()));
    assert!(
        ed.read_with(cx, |e, _| e.key_context_name())
            .contains("DatePicker")
    );
    assert_eq!(buffer(&ed, cx).as_deref(), Some("TODO task"));
    // The keyboard moves the highlighted day: right = +1, down = +1 week.
    cx.simulate_keystrokes("right down");
    assert_eq!(
        ed.read_with(cx, |e, _| e.picker_day()),
        Date::new(2025, 11, 22)
    );
    cx.simulate_keystrokes("left enter");
    assert!(ed.read_with(cx, |e, _| e.picker_state().is_none()));
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("TODO task\nSCHEDULED: <2025-11-21 Fri>")
    );
    // A deadline goes after the scheduled line; a click on a day (the calendar's own event)
    // picks it.
    cx.simulate_input(" /deadline");
    cx.simulate_keystrokes("enter");
    let state = ed.read_with(cx, |e, _| e.picker_state().cloned().expect("calendar"));
    state.update(cx, |s, cx| {
        crate::ui::calendar::activate(s, (2025, 12, 1), cx);
    });
    flush(&ed, cx);
    assert_eq!(
        env.disk(HOME),
        "- TODO task\n  SCHEDULED: <2025-11-21 Fri>\n  DEADLINE: <2025-12-01 Mon>\n"
    );
    // Escape closes the calendar and keeps the text.
    cx.simulate_input(" /date pi");
    assert_eq!(labels(&ed, cx)[0], "Date picker");
    cx.simulate_keystrokes("enter");
    assert!(ed.read_with(cx, |e, _| e.picker_state().is_some()));
    cx.simulate_keystrokes("escape");
    assert!(ed.read_with(cx, |e, _| e.picker_state().is_none()));
    assert_eq!(editing_row(&ed, cx), Some(0));
    cx.simulate_input(" /date pi");
    cx.simulate_keystrokes("enter enter");
    assert!(
        buffer(&ed, cx).as_deref().is_some_and(
            |t| t.starts_with("TODO task") && t.contains("[[Nov 14th, 2025]]\nSCHEDULED")
        ),
        "{:?}",
        buffer(&ed, cx)
    );
}

#[gpui_test]
fn upload_removes_the_trigger_and_asks_for_files(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" /upload");
    assert_eq!(labels(&ed, cx)[0], "Upload an asset");
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one"));
    assert_eq!(editing_row(&ed, cx), Some(0));
}
