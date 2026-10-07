//! Settings > Keymap: view the shortcuts, record a new one, resolve conflicts, reset
//! (BIT-T-0332). Changes are written to `keymap.json` and re-bound live.

use crate::views::dims;
use rust_i18n::t;

use super::keymap_model::{KeymapModel, KeymapRow, save_user_keymap};
use super::{Pending, SettingsView};
use crate::ui::button::Button;
use crate::ui::input::Input;
use crate::ui::theme::Theme;
use crate::ui::{
    AnyElement, ClickEvent, Context, Disableable as _, FluentBuilder as _, InteractiveElement as _,
    IntoElement, KeyDownEvent, Level, ParentElement as _, Sizable as _, Styled as _, Window, div,
    h_flex, v_flex,
};

/// Rows shown at once (the filter narrows the rest).
const MAX_ROWS: usize = 120;

/// Keys that only change modifiers; they never form a shortcut on their own.
fn is_modifier(key: &str) -> bool {
    matches!(
        key,
        "shift"
            | "control"
            | "ctrl"
            | "alt"
            | "option"
            | "cmd"
            | "command"
            | "super"
            | "meta"
            | "fn"
            | "platform"
            | "function"
    )
}

impl SettingsView {
    /// Rebuilds the model from the defaults, the user's `keymap.json` and the registered actions.
    pub(crate) fn reload_keymap(&mut self, cx: &mut Context<Self>) {
        let user = self
            .ctx
            .keymap_file
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        let registered: Vec<String> = cx
            .all_action_names()
            .iter()
            .filter(|n| n.starts_with("bitacora::") || n.starts_with("outliner::"))
            .map(|n| (*n).to_owned())
            .collect();
        self.keymap = KeymapModel::new(crate::keymap::DEFAULT_KEYMAP, user.as_deref(), &registered);
    }

    /// Starts listening for the new shortcut of an action.
    pub fn start_recording(
        &mut self,
        context: Option<String>,
        action: String,
        cx: &mut Context<Self>,
    ) {
        self.recording = Some((context, action));
        cx.notify();
    }

    pub(crate) fn record_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((context, action)) = self.recording.clone() else {
            return;
        };
        let stroke = &event.keystroke;
        if stroke.key == "escape" && !stroke.modifiers.modified() {
            self.recording = None;
            cx.notify();
            return;
        }
        if is_modifier(&stroke.key) {
            return;
        }
        self.recording = None;
        self.propose_binding(context, action, stroke.unparse(), window, cx);
    }

    /// Binds `keys` to `action`; a keystroke another action in the context uses is only taken
    /// after the user confirms.
    pub fn propose_binding(
        &mut self,
        context: Option<String>,
        action: String,
        keys: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conflicts = self.keymap.conflicts(&context, &action, &keys);
        if conflicts.is_empty() {
            self.bind_key(&context, &action, &keys, cx);
        } else {
            self.request(
                Pending::KeymapConflict {
                    context,
                    action,
                    keys,
                    conflicts,
                },
                window,
                cx,
            );
        }
    }

    /// Gives `action` the keystroke `keys` (taking it from a conflicting action), saves and
    /// re-binds live.
    pub(crate) fn bind_key(
        &mut self,
        context: &Option<String>,
        action: &str,
        keys: &str,
        cx: &mut Context<Self>,
    ) {
        self.keymap.rebind(context, action, keys);
        self.persist_keymap(cx);
    }

    /// Removes every keystroke of an action.
    pub fn clear_binding(
        &mut self,
        context: &Option<String>,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        self.keymap.unbind(context, action);
        self.persist_keymap(cx);
    }

    /// Restores the default keystrokes of an action.
    pub fn reset_binding(
        &mut self,
        context: &Option<String>,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        self.keymap.reset(context, action);
        self.persist_keymap(cx);
    }

    pub(crate) fn reset_keymap(&mut self, cx: &mut Context<Self>) {
        self.keymap.reset_all();
        self.persist_keymap(cx);
    }

    fn persist_keymap(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = &self.ctx.keymap_file
            && let Err(error) = save_user_keymap(path, self.keymap.user_json().as_deref())
        {
            self.say(
                Level::Error,
                t!("settings.keymap.save_failed", error = error.to_string()).to_string(),
                cx,
            );
            return;
        }
        let problems = crate::keymap::apply_live(cx, self.keymap.effective().clone());
        match problems.first() {
            Some(problem) => self.say(Level::Warning, problem.clone(), cx),
            None => self.say(
                Level::Success,
                t!("settings.keymap.applied").to_string(),
                cx,
            ),
        }
    }

    pub(crate) fn render_keymap(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let filter = self.inputs.keymap_filter.read(cx).value().to_string();
        let rows: Vec<KeymapRow> = self.keymap.rows(&filter);
        let total = rows.len();
        let customised = self.keymap.customised_count();
        let recording = self.recording.clone();

        let mut list = v_flex().id("settings-keymap-rows").gap_0p5();
        for (ix, row) in rows.into_iter().take(MAX_ROWS).enumerate() {
            let is_recording = recording
                .as_ref()
                .is_some_and(|(c, a)| *c == row.context && *a == row.action);
            let keys = if is_recording {
                t!("settings.keymap.press_keys").to_string()
            } else if row.keys.is_empty() {
                t!("settings.keymap.unbound").to_string()
            } else {
                row.keys.join(", ")
            };
            let (c1, a1) = (row.context.clone(), row.action.clone());
            let (c2, a2) = (row.context.clone(), row.action.clone());
            let (c3, a3) = (row.context.clone(), row.action.clone());
            let customised_row = row.customised();
            list = list.child(
                h_flex()
                    .id(("settings-keymap-row", ix))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .items_center()
                    .rounded(dims::PX_4)
                    .when(is_recording, |r| r.bg(theme.accent.opacity(0.25)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().child(row.short_name().to_owned()))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!(
                                    "{} \u{b7} {}",
                                    row.context.as_deref().unwrap_or("global"),
                                    row.action
                                ),
                            )),
                    )
                    .child(
                        div()
                            .w(dims::PX_200)
                            .text_sm()
                            .font_family("monospace")
                            .when(customised_row, |d| d.text_color(theme.warning))
                            .child(keys),
                    )
                    .child(
                        Button::new(("settings-key-record", ix))
                            .xsmall()
                            .label(t!("settings.keymap.record").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.start_recording(c1.clone(), a1.clone(), cx);
                            })),
                    )
                    .child(
                        Button::new(("settings-key-clear", ix))
                            .xsmall()
                            .disabled(row.keys.is_empty())
                            .label(t!("settings.keymap.clear").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.clear_binding(&c2, &a2, cx);
                            })),
                    )
                    .child(
                        Button::new(("settings-key-reset", ix))
                            .xsmall()
                            .disabled(!customised_row)
                            .label(t!("settings.keymap.reset").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.reset_binding(&c3, &a3, cx);
                            })),
                    ),
            );
        }
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&self.inputs.keymap_filter)))
                    .child(
                        Button::new("settings-keymap-reset-all")
                            .small()
                            .disabled(customised == 0)
                            .label(t!("settings.keymap.reset_all", count = customised).to_string())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.request(Pending::ResetKeymap, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("settings.keymap.help", count = total).to_string()),
            )
            .child(list)
            .into_any_element()
    }
}
