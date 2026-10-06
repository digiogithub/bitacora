//! The credential prompt (BIT-US-0046, BIT-T-0291): a modal asking for a username and
//! password / token, or an SSH key passphrase, whenever git needs one and the OS keyring has
//! nothing. Requests arrive from the sync engine and onboarding threads through
//! [`crate::credentials::PromptRequest`]; cancelling (or closing the app) answers "no
//! credential", which the engine turns into an authentication error state without retrying.

use std::collections::VecDeque;

use bitacora_sync::credentials::{
    Credential, CredentialKind, CredentialRequest, PromptAnswer, Secret,
};
use rust_i18n::t;

use crate::credentials::{PromptRequest, prompt_kind_key};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputEvent, InputState};
use crate::ui::{
    ActiveTheme as _, AppContext as _, Context, Entity, FluentBuilder as _, Focusable as _,
    IconName, IntoElement, ParentElement as _, Render, Sizable as _, Styled as _, Subscription,
    Window, div, h_flex, px, v_flex,
};
use crate::views::modal::{labelled, modal, title_bar};

/// The modal. Requests queue up; the front one is shown.
pub struct CredentialDialog {
    queue: VecDeque<PromptRequest>,
    username: Entity<InputState>,
    secret: Entity<InputState>,
    remember: bool,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for CredentialDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialDialog")
            .field("pending", &self.queue.len())
            .finish_non_exhaustive()
    }
}

impl CredentialDialog {
    /// A closed dialog.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let username = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("credentials.username").to_string())
        });
        let secret = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(t!("credentials.secret").to_string())
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &username,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        let focus = this.secret.read(cx).focus_handle(cx);
                        window.focus(&focus, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &secret,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.accept(window, cx);
                    }
                },
            ),
        ];
        Self {
            queue: VecDeque::new(),
            username,
            secret,
            remember: true,
            _subscriptions: subscriptions,
        }
    }

    /// Queues a request; shows it right away when none is open.
    pub fn ask(&mut self, request: PromptRequest, window: &mut Window, cx: &mut Context<Self>) {
        let first = self.queue.is_empty();
        self.queue.push_back(request);
        if first {
            self.show_front(window, cx);
        }
        cx.notify();
    }

    /// The request being shown.
    pub fn current(&self) -> Option<&CredentialRequest> {
        self.queue.front().map(|r| &r.request)
    }

    /// How many requests wait (including the one shown).
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Whether "remember" is on.
    pub fn remember(&self) -> bool {
        self.remember
    }

    fn show_front(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(front) = self.queue.front() else {
            return;
        };
        let (known_user, kind) = (front.request.username.clone(), front.request.kind);
        self.username.update(cx, |i, cx| {
            i.set_value(known_user.unwrap_or_default(), window, cx);
        });
        self.secret.update(cx, |i, cx| i.set_value("", window, cx));
        self.remember = true;
        // A passphrase has no user name; start in the field that is needed.
        let target = match kind {
            CredentialKind::SshPassphrase => &self.secret,
            CredentialKind::UserPassword => &self.username,
        };
        let focus = target.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Sets the "remember" choice.
    pub fn set_remember(&mut self, remember: bool, cx: &mut Context<Self>) {
        self.remember = remember;
        cx.notify();
    }

    /// Answers the shown request with what is typed in.
    pub fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.queue.pop_front() else {
            return;
        };
        let username = match request.request.kind {
            CredentialKind::SshPassphrase => String::new(),
            CredentialKind::UserPassword => self.username.read(cx).value().to_string(),
        };
        let secret = self.secret.read(cx).value().to_string();
        request.answer(Some(PromptAnswer {
            credential: Credential {
                username,
                secret: Secret::new(secret),
            },
            remember: self.remember,
        }));
        self.secret.update(cx, |i, cx| i.set_value("", window, cx));
        self.show_front(window, cx);
        cx.notify();
    }

    /// Cancels the shown request: the engine gets "no credential".
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.queue.pop_front() else {
            return;
        };
        request.answer(None);
        self.secret.update(cx, |i, cx| i.set_value("", window, cx));
        self.show_front(window, cx);
        cx.notify();
    }
}

impl Render for CredentialDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(front) = self.queue.front() else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let this = cx.entity();
        let kind = front.request.kind;
        let target = front.request.url.clone();

        let mut body = v_flex().gap_3().p_4().child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(t!("credentials.intro", target = target).to_string()),
        );
        if kind == CredentialKind::UserPassword {
            body = body.child(labelled(
                &theme,
                t!("credentials.username").to_string(),
                Input::new(&self.username),
            ));
        }
        let secret_label = match kind {
            CredentialKind::UserPassword => t!("credentials.password"),
            CredentialKind::SshPassphrase => t!("credentials.passphrase"),
        }
        .to_string();
        let remember_this = this.clone();
        body =
            body.child(labelled(&theme, secret_label, Input::new(&self.secret)))
                .child(
                    Button::new("credentials-remember")
                        .ghost()
                        .small()
                        .icon(if self.remember {
                            IconName::Check
                        } else {
                            IconName::Square
                        })
                        .label(t!("credentials.remember").to_string())
                        .on_click(move |_, _, cx| {
                            remember_this.update(cx, |d, cx| {
                                let next = !d.remember;
                                d.set_remember(next, cx);
                            });
                        }),
                )
                .when(self.queue.len() > 1, |col| {
                    col.child(
                        div().text_xs().text_color(theme.muted_foreground).child(
                            t!("credentials.more", count = self.queue.len() - 1).to_string(),
                        ),
                    )
                });

        let accept = this.clone();
        let cancel = this.clone();
        let dismiss = this.clone();
        let footer = h_flex()
            .px_4()
            .py_3()
            .gap_2()
            .justify_end()
            .border_t_1()
            .border_color(theme.border)
            .child(
                Button::new("credentials-cancel")
                    .label(t!("credentials.cancel").to_string())
                    .on_click(move |_, window, cx| {
                        cancel.update(cx, |d, cx| d.cancel(window, cx));
                    }),
            )
            .child(
                Button::new("credentials-ok")
                    .primary()
                    .label(t!("credentials.ok").to_string())
                    .on_click(move |_, window, cx| {
                        accept.update(cx, |d, cx| d.accept(window, cx));
                    }),
            );
        let title = t!(prompt_kind_key(kind)).to_string();
        modal(
            "credential-dialog",
            &theme,
            460.,
            // Clicking outside is not a cancel: a stray click must not fail a sync.
            move |_, _| {
                let _ = &dismiss;
            },
            v_flex()
                .child(title_bar(&theme, title, div().w(px(0.))))
                .child(body)
                .child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::{settings::AppSettings, theme};

    type Reply = std::sync::mpsc::Receiver<Option<PromptAnswer>>;

    fn request(kind: CredentialKind, user: Option<&str>) -> (PromptRequest, Reply) {
        PromptRequest::for_test(CredentialRequest {
            url: "https://example.org/notes".into(),
            username: user.map(str::to_owned),
            kind,
        })
    }

    #[gpui_test]
    fn requests_queue_and_are_answered_or_cancelled_in_order(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
        let (dialog, cx) = cx.add_window_view(CredentialDialog::new);
        let (first, first_rx) = request(CredentialKind::UserPassword, Some("ana"));
        let (second, second_rx) = request(CredentialKind::SshPassphrase, None);
        dialog.update_in(cx, |d, window, cx| {
            d.ask(first, window, cx);
            d.ask(second, window, cx);
        });
        assert_eq!(dialog.read_with(cx, |d, _| d.pending()), 2);
        // The known user name is prefilled; type the secret and accept.
        dialog.update_in(cx, |d, window, cx| {
            d.secret
                .update(cx, |i, cx| i.set_value("s3cret", window, cx));
            d.set_remember(false, cx);
            d.accept(window, cx);
        });
        let answer = first_rx.recv().expect("answer").expect("credential");
        assert_eq!(answer.credential.username, "ana");
        assert_eq!(answer.credential.secret.expose(), "s3cret");
        assert!(!answer.remember);
        // The passphrase request is next; cancelling answers "no credential".
        assert_eq!(
            dialog.read_with(cx, |d, _| d.current().map(|r| r.kind)),
            Some(CredentialKind::SshPassphrase)
        );
        assert!(
            dialog.read_with(cx, |d, _| d.remember()),
            "remember resets per request"
        );
        dialog.update_in(cx, |d, window, cx| d.cancel(window, cx));
        assert!(second_rx.recv().expect("answer").is_none());
        assert_eq!(dialog.read_with(cx, |d, _| d.pending()), 0);
    }
}
