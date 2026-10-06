//! The sync panel: status, backend, preferences and the command line (BIT-US-0047,
//! BIT-T-0294, BIT-T-0295, BIT-T-0375).
//!
//! It opens from the status bar's sync slot and from the palette ("Sync settings..."). It shows
//! what the sync engine publishes ([`SyncStatusView`]: message, retry flag, ahead/behind, last
//! sync, last error), the active git backend with the install-git suggestion, the saved sync
//! preferences with enable / turn off, and the `bitacora-cli` commands that do the same
//! headlessly. Actions leave through [`SyncPanelEvent`]; the workspace performs them.

use std::path::PathBuf;
use std::time::SystemTime;

use bitacora_runtime::SyncStatusView;
use bitacora_sync::backend::ActiveBackend;
use rust_i18n::t;

use crate::sync_prefs::SyncPrefs;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, Context, Disableable as _, EventEmitter, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Sizable as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, h_flex, icon, px, v_flex,
};
use crate::views::modal::{labelled, modal, title_bar};
use crate::views::status_bar::{SlotState, slot_state_for};

/// What the panel asks the workspace to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPanelEvent {
    /// Commit, fetch, merge and push now.
    SyncNow,
    /// Open the "Enable sync" form.
    Enable,
    /// Turn background sync off for this graph.
    Disable,
    /// Open the conflict resolver.
    OpenConflicts,
    /// Open the history of the page on screen.
    OpenHistory,
    /// The panel closed.
    Closed,
}

/// "5 minutes ago" style text for the last sync.
pub fn ago(time: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(time).map_or(0, |d| d.as_secs());
    match secs {
        0..=59 => t!("sync.panel.ago_now").to_string(),
        60..=3599 => t!("sync.panel.ago_minutes", count = secs / 60).to_string(),
        3600..=86_399 => t!("sync.panel.ago_hours", count = secs / 3600).to_string(),
        _ => t!("sync.panel.ago_days", count = secs / 86_400).to_string(),
    }
}

/// The overlay.
#[derive(Debug, Default)]
pub struct SyncPanel {
    open: bool,
    view: Option<SyncStatusView>,
    prefs: SyncPrefs,
    graph: Option<PathBuf>,
    askpass: bool,
    history_available: bool,
}

impl EventEmitter<SyncPanelEvent> for SyncPanel {}

impl SyncPanel {
    /// A closed panel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the panel is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows the panel.
    pub fn show(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        cx.notify();
    }

    /// Hides the panel.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.emit(SyncPanelEvent::Closed);
            cx.notify();
        }
    }

    /// The latest engine status (`None` while sync is off).
    pub fn set_view(&mut self, view: Option<SyncStatusView>, cx: &mut Context<Self>) {
        self.view = view;
        cx.notify();
    }

    /// The saved preferences of the open graph.
    pub fn set_prefs(
        &mut self,
        graph: Option<PathBuf>,
        prefs: SyncPrefs,
        askpass: bool,
        cx: &mut Context<Self>,
    ) {
        self.graph = graph;
        self.prefs = prefs;
        self.askpass = askpass;
        cx.notify();
    }

    /// Whether a page is on screen (enables "Page history").
    pub fn set_history_available(&mut self, available: bool, cx: &mut Context<Self>) {
        self.history_available = available;
        cx.notify();
    }

    /// The status shown.
    pub fn view(&self) -> Option<&SyncStatusView> {
        self.view.as_ref()
    }
}

impl Render for SyncPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let this = cx.entity();
        let dismiss = this.clone();
        let emit = |event: SyncPanelEvent| {
            let this = this.clone();
            move |_: &crate::ui::ClickEvent, _: &mut Window, cx: &mut crate::ui::App| {
                this.update(cx, |_, cx| cx.emit(event));
            }
        };

        // Status.
        let mut status = v_flex().gap_2();
        match &self.view {
            Some(view) => {
                let state = slot_state_for(view);
                let color = match state {
                    SlotState::Error => theme.danger,
                    SlotState::Busy => theme.info,
                    _ => theme.success,
                };
                status = status.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            icon(match state {
                                SlotState::Off => IconName::Minus,
                                SlotState::Idle => IconName::CircleCheck,
                                SlotState::Busy => IconName::LoaderCircle,
                                SlotState::Error => IconName::CircleAlert,
                            })
                            .text_color(color),
                        )
                        .child(div().id("sync-panel-message").child(view.message.clone())),
                );
                let mut facts = Vec::new();
                facts.push(match view.status.last_sync {
                    Some(t) => {
                        t!("sync.panel.last_sync", when = ago(t, SystemTime::now())).to_string()
                    }
                    None => t!("sync.panel.never_synced").to_string(),
                });
                facts.push(
                    t!(
                        "sync.panel.ahead_behind",
                        ahead = view.status.ahead,
                        behind = view.status.behind
                    )
                    .to_string(),
                );
                status = status.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(facts.join(" \u{b7} ")),
                );
                if let Some(error) = &view.status.last_error {
                    status = status.child(
                        div()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(error.clone()),
                    );
                }
                if view.status.conflicts > 0 {
                    status = status.child(
                        Button::new("sync-panel-conflicts")
                            .small()
                            .icon(IconName::TriangleAlert)
                            .label(
                                t!(
                                    "sync.panel.resolve_conflicts",
                                    count = view.status.conflicts
                                )
                                .to_string(),
                            )
                            .on_click(emit(SyncPanelEvent::OpenConflicts)),
                    );
                }
                status = status.child(
                    h_flex().gap_2().child(
                        Button::new("sync-panel-now")
                            .primary()
                            .small()
                            .icon(IconName::RefreshCw)
                            .label(if view.can_retry {
                                t!("sync.panel.retry").to_string()
                            } else {
                                t!("sync.panel.sync_now").to_string()
                            })
                            .on_click(emit(SyncPanelEvent::SyncNow)),
                    ),
                );
            }
            None => {
                status = status.child(div().text_sm().text_color(theme.muted_foreground).child(
                    if self.prefs.enabled {
                        t!("sync.panel.starting").to_string()
                    } else {
                        t!("sync.panel.off").to_string()
                    },
                ));
            }
        }

        // Backend.
        let backend = self.view.as_ref().map(|view| {
            let hint = view
                .hint
                .clone()
                .or_else(|| view.backend.install_git_suggestion.map(str::to_owned));
            (view.backend.description.clone(), view.backend.kind, hint)
        });
        let backend_section = backend.map(|(description, kind, hint)| {
            v_flex()
                .gap_1()
                .child(labelled(
                    &theme,
                    t!("sync.panel.backend").to_string(),
                    div()
                        .id("sync-panel-backend")
                        .child(description)
                        .into_any_element(),
                ))
                .when(kind == ActiveBackend::GixOnly || hint.is_some(), |col| {
                    col.children(hint.map(|hint| {
                        div()
                            .id("sync-panel-hint")
                            .text_xs()
                            .text_color(theme.warning)
                            .child(hint)
                    }))
                })
        });

        // Preferences.
        let prefs = &self.prefs;
        let remote = if prefs.remote_url.is_empty() {
            t!("sync.panel.none").to_string()
        } else {
            prefs.remote_url.clone()
        };
        let identity = match (&prefs.author_name, &prefs.author_email) {
            (Some(n), Some(e)) => format!("{n} <{e}>"),
            _ => t!("sync.panel.identity_default").to_string(),
        };
        let settings = v_flex()
            .gap_2()
            .child(labelled(
                &theme,
                t!("sync.panel.remote").to_string(),
                remote,
            ))
            .child(
                h_flex()
                    .gap_4()
                    .child(labelled(
                        &theme,
                        t!("sync.panel.branch").to_string(),
                        prefs.branch.clone(),
                    ))
                    .child(labelled(
                        &theme,
                        t!("sync.panel.device").to_string(),
                        prefs.device.clone(),
                    )),
            )
            .child(labelled(
                &theme,
                t!("sync.panel.identity").to_string(),
                identity,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("sync-panel-enable")
                            .small()
                            .disabled(self.graph.is_none())
                            .label(if prefs.enabled {
                                t!("sync.panel.change").to_string()
                            } else {
                                t!("sync.panel.enable").to_string()
                            })
                            .on_click(emit(SyncPanelEvent::Enable)),
                    )
                    .when(prefs.enabled, |row| {
                        row.child(
                            Button::new("sync-panel-disable")
                                .small()
                                .label(t!("sync.panel.disable").to_string())
                                .on_click(emit(SyncPanelEvent::Disable)),
                        )
                    }),
            );

        let credentials_line = if self.askpass {
            t!("sync.panel.askpass_on").to_string()
        } else {
            t!("sync.panel.askpass_off").to_string()
        };

        // Command line.
        let graph_arg = self
            .graph
            .as_ref()
            .map_or_else(|| "<graph>".to_owned(), |g| g.display().to_string());
        let cli = v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("sync.panel.cli_intro").to_string()),
            )
            .children(
                [
                    format!("bitacora-cli sync --graph \"{graph_arg}\""),
                    format!("bitacora-cli doctor --graph \"{graph_arg}\""),
                ]
                .into_iter()
                .enumerate()
                .map(|(n, line)| {
                    div()
                        .id(("sync-panel-cli", n))
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .bg(theme.secondary)
                        .text_xs()
                        .font_family("monospace")
                        .child(line)
                }),
            );

        let close = this.clone();
        let history_btn = Button::new("sync-panel-history")
            .small()
            .icon(IconName::Calendar)
            .disabled(!self.history_available)
            .label(t!("sync.panel.history").to_string())
            .on_click(emit(SyncPanelEvent::OpenHistory));

        let body = v_flex()
            .id("sync-panel-body")
            .gap_4()
            .p_4()
            .overflow_y_scroll()
            .max_h(px(560.))
            .child(status)
            .children(backend_section)
            .child(settings)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(credentials_line),
            )
            .child(history_btn)
            .child(cli);

        modal(
            "sync-panel",
            &theme,
            560.,
            move |_, cx| dismiss.update(cx, |p, cx| p.close(cx)),
            v_flex()
                .child(title_bar(
                    &theme,
                    t!("sync.panel.title").to_string(),
                    Button::new("sync-panel-close")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| close.update(cx, |p, cx| p.close(cx))),
                ))
                .child(body),
        )
    }
}
