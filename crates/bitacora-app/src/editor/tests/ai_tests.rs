//! `#[gpui::test]` suite of the AI help in the editor (BIT-US-0153): the inline ghost text and
//! the compose box, against a fake backend. Same setup as the parent suite: a real graph, a live
//! session and simulated keystrokes.

use async_channel::Sender;
use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::editor::ai::{
    self, AiBackend, AiEvent, AiRun, ComposeMode, ComposePhase, ComposeRequest, GHOST_DELAY,
};
use crate::ui::App;

/// Records requests and lets the test answer them.
#[derive(Default)]
struct Fake {
    requests: RefCell<Vec<ComposeRequest>>,
    senders: RefCell<Vec<Sender<AiEvent>>>,
}

impl AiBackend for Rc<Fake> {
    fn start(&self, _: &App, request: ComposeRequest) -> AiRun {
        let (tx, rx) = async_channel::unbounded();
        self.requests.borrow_mut().push(request);
        self.senders.borrow_mut().push(tx);
        AiRun::new(rx, ())
    }
}

/// Installs a fake backend and, when asked, turns the two switches on.
fn install(cx: &mut VisualTestContext, ghost: bool, compose: bool) -> Rc<Fake> {
    let fake = Rc::new(Fake::default());
    let backend: Rc<dyn AiBackend> = Rc::new(fake.clone());
    cx.update(|_, cx| {
        ai::install(cx, Some(backend));
        theme::edit_settings(cx, None, |s| {
            s.ai_assist.ghost_text = ghost;
            s.ai_assist.compose = compose;
        });
    });
    fake
}

const TEXT: &str = "The quick brown fox jumps";

fn wait_ghost_delay(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(GHOST_DELAY);
    cx.run_until_parked();
}

fn answer(fake: &Fake, event: AiEvent, cx: &mut VisualTestContext) {
    let tx = fake.senders.borrow().last().cloned().expect("a run");
    tx.try_send(event).expect("send");
    cx.run_until_parked();
}

fn ghost(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) -> Option<String> {
    ed.read_with(cx, |e, _| e.ghost().map(str::to_owned))
}

#[gpui_test]
fn ghost_text_is_off_by_default(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, false, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" over");
    wait_ghost_delay(cx);
    assert!(fake.requests.borrow().is_empty(), "nothing is requested");
    assert_eq!(ghost(&ed, cx), None);
}

#[gpui_test]
fn a_pause_requests_a_continuation_and_tab_accepts_it_as_one_undoable_edit(
    cx: &mut TestAppContext,
) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" over");
    // Typing alone requests nothing: the request waits for a quiet moment.
    assert!(fake.requests.borrow().is_empty());
    wait_ghost_delay(cx);
    {
        let requests = fake.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].mode, ComposeMode::Continue);
        assert_eq!(requests[0].page, "Home");
        assert_eq!(requests[0].block_text, format!("{TEXT} over"));
    }
    answer(&fake, AiEvent::Done("the lazy dog.".into()), cx);
    assert_eq!(ghost(&ed, cx).as_deref(), Some(" the lazy dog."));
    // The suggestion is not in the buffer nor in the graph until it is accepted.
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("The quick brown fox jumps over")
    );
    flush(&ed, cx);
    assert_eq!(
        env.snapshot_texts("Home")[0].1,
        "The quick brown fox jumps over"
    );

    cx.simulate_keystrokes("tab");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("The quick brown fox jumps over the lazy dog.")
    );
    assert_eq!(ghost(&ed, cx), None);
    assert_eq!(editing_row(&ed, cx), Some(0), "Tab did not indent or leave");
    assert_eq!(env.snapshot_texts("Home")[0].0, 1);
    flush(&ed, cx);
    assert_eq!(
        env.disk(HOME),
        "- The quick brown fox jumps over the lazy dog.\n"
    );
    // Accepting is a normal edit: one undo step takes it back.
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(
        env.snapshot_texts("Home")[0].1,
        "The quick brown fox jumps over",
        "one undo step takes back the suggestion and only it"
    );
    assert_eq!(env.disk(HOME), "- The quick brown fox jumps over\n");
    // Accepting did not chain another request.
    wait_ghost_delay(cx);
    assert_eq!(fake.requests.borrow().len(), 1);
}

#[gpui_test]
fn escape_dismisses_the_ghost_and_keeps_editing(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    answer(&fake, AiEvent::Done(" and more".into()), cx);
    assert!(ghost(&ed, cx).is_some());
    cx.simulate_keystrokes("escape");
    assert_eq!(ghost(&ed, cx), None);
    assert_eq!(
        editing_row(&ed, cx),
        Some(0),
        "the first Escape only dismissed"
    );
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("The quick brown fox jumps!")
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        editing_row(&ed, cx),
        None,
        "the second Escape leaves the block"
    );
}

#[gpui_test]
fn typing_and_caret_moves_discard_the_ghost_and_cancel_the_request(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    let first = fake.senders.borrow()[0].clone();
    assert!(!first.is_closed(), "the run is alive");
    // A keystroke while the request is in flight cancels it.
    cx.simulate_input("?");
    assert!(first.is_closed(), "typing dropped the run");
    // A late answer for an old buffer is ignored.
    assert!(first.try_send(AiEvent::Done(" stale".into())).is_err());
    cx.run_until_parked();
    assert_eq!(ghost(&ed, cx), None);

    wait_ghost_delay(cx);
    assert_eq!(fake.requests.borrow().len(), 2);
    answer(&fake, AiEvent::Done(" next".into()), cx);
    assert!(ghost(&ed, cx).is_some());
    cx.simulate_keystrokes("left");
    assert_eq!(ghost(&ed, cx), None, "moving the caret discards it");
    cx.simulate_keystrokes("end");
    assert_eq!(ghost(&ed, cx), None);
    // Typing over a shown ghost replaces it with nothing.
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    answer(&fake, AiEvent::Done(" again".into()), cx);
    assert!(ghost(&ed, cx).is_some());
    cx.simulate_input("x");
    assert_eq!(ghost(&ed, cx), None);
}

#[gpui_test]
fn no_request_happens_during_ime_composition_and_a_composition_hides_the_ghost(
    cx: &mut TestAppContext,
) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, false);
    edit(&ed, 0, Caret::End, cx);
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "kan", Some(3..3), window, cx);
    });
    wait_ghost_delay(cx);
    assert!(
        fake.requests.borrow().is_empty(),
        "nothing is requested while composing"
    );
    // Commit the composition; the next pause requests as usual.
    ed.update_in(cx, |e, window, cx| {
        e.replace_text_in_range(None, "kan", window, cx)
    });
    wait_ghost_delay(cx);
    assert_eq!(fake.requests.borrow().len(), 1);
    answer(&fake, AiEvent::Done(" tail".into()), cx);
    assert!(ghost(&ed, cx).is_some());
    // A new composition makes the shown ghost void at once, before any other key.
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "x", Some(1..1), window, cx);
    });
    assert_eq!(ghost(&ed, cx), None);
    cx.simulate_keystrokes("tab");
    assert_eq!(
        env.snapshot_texts("Home")[0].0,
        1,
        "Tab did not accept anything during composition"
    );
}

#[gpui_test]
fn short_and_property_text_requests_nothing(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- hi\n- status:: a rather long value here\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    assert!(fake.requests.borrow().is_empty());
}

#[gpui_test]
fn a_failed_request_pauses_further_requests(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    answer(&fake, AiEvent::Failed("not connected".into()), cx);
    cx.simulate_input("?");
    wait_ghost_delay(cx);
    assert_eq!(
        fake.requests.borrow().len(),
        1,
        "backed off after the failure"
    );
    assert_eq!(ghost(&ed, cx), None);
}

#[gpui_test]
fn a_private_block_is_never_sent(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, &format!("- {TEXT}\n  private:: true\n- {TEXT}\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, true, true);
    // The caret is on the title line: only the property line itself would make a ghost
    // impossible, the privacy rule must hold on its own.
    edit(&ed, 0, Caret::Start, cx);
    cx.simulate_input("!");
    wait_ghost_delay(cx);
    assert!(fake.requests.borrow().is_empty(), "no ghost request");
    cx.simulate_keystrokes(&k("ctrl-j"));
    assert!(!ed.read_with(cx, |e, _| e.compose_open()), "no compose box");
}

fn open_compose(ed: &Entity<OutlineEditor>, instruction: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&k("ctrl-j"));
    assert!(
        ed.read_with(cx, |e, _| e.compose_open()),
        "Ctrl/Cmd+J opens the box"
    );
    let input = ed.read_with(cx, |e, _| e.compose_input()).expect("input");
    input.update_in(cx, |i, window, cx| {
        i.set_value(instruction.to_owned(), window, cx)
    });
    cx.run_until_parked();
}

#[gpui_test]
fn compose_streams_a_draft_and_insert_below_is_one_undo_step(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- first\n- second\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, false, true);
    edit(&ed, 0, Caret::End, cx);
    open_compose(&ed, "make it shouty", cx);
    ed.update_in(cx, |e, window, cx| e.compose_submit(window, cx));
    cx.run_until_parked();
    {
        let requests = fake.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].mode, ComposeMode::Compose);
        assert_eq!(requests[0].instruction, "make it shouty");
        assert_eq!(requests[0].block_text, "first");
        assert!(requests[0].include_block);
    }
    assert_eq!(
        ed.read_with(cx, |e, _| e.compose_phase().cloned()),
        Some(ComposePhase::Running)
    );
    answer(&fake, AiEvent::Preview("FIR".into()), cx);
    assert_eq!(
        ed.read_with(cx, |e, _| e.compose_draft().map(str::to_owned))
            .as_deref(),
        Some("FIR")
    );
    // The draft never touches the graph while it is a draft.
    assert_eq!(env.snapshot_texts("Home").len(), 2);
    answer(&fake, AiEvent::Done("FIRST!".into()), cx);
    assert_eq!(
        ed.read_with(cx, |e, _| e.compose_phase().cloned()),
        Some(ComposePhase::Ready)
    );
    assert_eq!(env.snapshot_texts("Home").len(), 2);

    ed.update_in(cx, |e, window, cx| e.compose_insert_below(window, cx));
    cx.run_until_parked();
    assert!(!ed.read_with(cx, |e, _| e.compose_open()));
    let texts: Vec<_> = env
        .snapshot_texts("Home")
        .into_iter()
        .map(|t| t.1)
        .collect();
    assert_eq!(texts, ["first", "FIRST!", "second"]);
    assert_eq!(env.disk(HOME), "- first\n- FIRST!\n- second\n");
    // One undo step removes the whole insertion.
    cx.simulate_keystrokes(&k("ctrl-z"));
    let texts: Vec<_> = env
        .snapshot_texts("Home")
        .into_iter()
        .map(|t| t.1)
        .collect();
    assert_eq!(texts, ["first", "second"]);
    assert_eq!(env.disk(HOME), "- first\n- second\n");
}

#[gpui_test]
fn compose_replace_is_one_undo_step_and_discard_changes_nothing(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- first\n- second\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, false, true);
    edit(&ed, 0, Caret::End, cx);
    // Discard: the block and the file stay as they were, also after a finished draft.
    open_compose(&ed, "rewrite", cx);
    ed.update_in(cx, |e, window, cx| e.compose_submit(window, cx));
    cx.run_until_parked();
    answer(&fake, AiEvent::Done("rewritten".into()), cx);
    ed.update_in(cx, |e, window, cx| e.compose_discard(window, cx));
    cx.run_until_parked();
    assert!(!ed.read_with(cx, |e, _| e.compose_open()));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("first"));
    flush(&ed, cx);
    assert_eq!(env.disk(HOME), "- first\n- second\n");
    assert_eq!(editing_row(&ed, cx), Some(0), "editing continues");

    // Replace.
    open_compose(&ed, "rewrite", cx);
    ed.update_in(cx, |e, window, cx| e.compose_submit(window, cx));
    cx.run_until_parked();
    answer(&fake, AiEvent::Done("rewritten".into()), cx);
    ed.update_in(cx, |e, window, cx| e.compose_replace(window, cx));
    cx.run_until_parked();
    assert_eq!(env.snapshot_texts("Home")[0].1, "rewritten");
    assert_eq!(env.disk(HOME), "- rewritten\n- second\n");
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(
        env.snapshot_texts("Home")[0].1,
        "first",
        "one undo step restores the block"
    );
    assert_eq!(env.disk(HOME), "- first\n- second\n");
}

#[gpui_test]
fn compose_failures_are_shown_and_retry_asks_again(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- first\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, false, true);
    edit(&ed, 0, Caret::End, cx);
    open_compose(&ed, "go", cx);
    ed.update_in(cx, |e, window, cx| e.compose_submit(window, cx));
    cx.run_until_parked();
    answer(&fake, AiEvent::Failed("this page is excluded".into()), cx);
    assert_eq!(
        ed.read_with(cx, |e, _| e.compose_phase().cloned()),
        Some(ComposePhase::Failed("this page is excluded".into()))
    );
    // Nothing can be inserted from a failed run.
    ed.update_in(cx, |e, window, cx| e.compose_insert_below(window, cx));
    assert_eq!(env.snapshot_texts("Home").len(), 1);
    ed.update_in(cx, |e, window, cx| e.compose_submit(window, cx));
    cx.run_until_parked();
    assert_eq!(fake.requests.borrow().len(), 2);
    // Escape closes the box without touching the block.
    cx.simulate_keystrokes("escape");
    assert!(!ed.read_with(cx, |e, _| e.compose_open()));
    assert_eq!(env.snapshot_texts("Home")[0].1, "first");
}

#[gpui_test]
fn compose_is_unavailable_until_switched_on(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- first\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let fake = install(cx, false, false);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-j"));
    assert!(!ed.read_with(cx, |e, _| e.compose_open()));
    assert!(fake.requests.borrow().is_empty());
    assert_eq!(
        editing_row(&ed, cx),
        Some(0),
        "the block is still being edited"
    );
}
