//! Probe for BIT-T-0105 (GPUI Kit `Textarea` per block, ADR-002 Option A): can an
//! ancestor intercept the edge keys needed for cross-block navigation?
//!
//! The assertions pin the *observed* behaviour of gpui-kit 0.7.1 so a kit upgrade that
//! changes it shows up here.

use crate::ui::input::{
    Backspace, Enter, Indent, InputEvent, MoveDown, MoveUp, Textarea, TextareaState,
};
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::ui::{
    App, AppContext as _, Context, Entity, FluentBuilder as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Window, div,
};

struct Probe {
    state: Entity<TextareaState>,
    log: Vec<String>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Probe")
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.log.push("capture:up".into());
                cx.propagate();
            }))
            .on_action(cx.listener(|this, _: &MoveUp, _, _| this.log.push("bubble:up".into())))
            .on_action(cx.listener(|this, _: &MoveDown, _, _| this.log.push("bubble:down".into())))
            .on_action(cx.listener(|this, _: &Backspace, _, _| {
                this.log.push("bubble:backspace".into());
            }))
            .on_action(cx.listener(|this, _: &Indent, _, _| this.log.push("bubble:tab".into())))
            .on_action(cx.listener(|this, _: &Enter, _, _| this.log.push("bubble:enter".into())))
            .child(Textarea::new(&self.state))
            .when(false, |d| d)
    }
}

fn open<'a>(cx: &'a mut TestAppContext, text: &str) -> (Entity<Probe>, &'a mut VisualTestContext) {
    cx.update(|cx: &mut App| crate::ui::init(cx));
    let text = text.to_owned();
    let (probe, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| TextareaState::new(window, cx));
        state.update(cx, |s, cx| s.set_value(text, window, cx));
        Probe {
            state,
            log: Vec::new(),
        }
    });
    probe.update_in(cx, |probe, window, cx| {
        probe.state.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.run_until_parked();
    (probe, cx)
}

fn log(probe: &Entity<Probe>, cx: &mut VisualTestContext) -> Vec<String> {
    probe.update(cx, |p, _| std::mem::take(&mut p.log))
}

#[gpui_test]
fn up_is_visible_to_a_capturing_ancestor_but_is_consumed_by_the_textarea(cx: &mut TestAppContext) {
    let (probe, cx) = open(cx, "first\nsecond");
    cx.simulate_keystrokes("up");
    let seen = log(&probe, cx);
    assert!(seen.contains(&"capture:up".to_owned()), "{seen:?}");
    assert!(!seen.contains(&"bubble:up".to_owned()), "{seen:?}");
}

#[gpui_test]
fn backspace_at_offset_zero_propagates_to_the_ancestor(cx: &mut TestAppContext) {
    let (probe, cx) = open(cx, "abc");
    cx.simulate_keystrokes("home");
    log(&probe, cx);
    cx.simulate_keystrokes("backspace");
    assert_eq!(log(&probe, cx), ["bubble:backspace"]);
    // In the middle of the text it is consumed.
    cx.simulate_keystrokes("right backspace");
    assert!(log(&probe, cx).is_empty());
}

#[gpui_test]
fn enter_and_tab_behaviour(cx: &mut TestAppContext) {
    let (probe, cx) = open(cx, "abc");
    cx.simulate_keystrokes("enter");
    let enter = log(&probe, cx);
    cx.simulate_keystrokes("tab");
    let tab = log(&probe, cx);
    // A multi-line Textarea keeps Enter and Tab for itself (Enter only propagates and
    // emits `InputEvent::PressEnter` when `submit_on_enter` is set); an ancestor needs
    // `capture_action` to see them.
    assert!(!enter.contains(&"bubble:enter".to_owned()), "{enter:?}");
    assert_eq!(
        tab,
        Vec::<String>::new(),
        "Tab is consumed by the Textarea (inline indent); an ancestor must capture it"
    );
    let _ = InputEvent::Change;
}
