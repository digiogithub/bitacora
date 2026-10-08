//! Pando status surfaces of the workspace (BIT-US-0140, BIT-T-0429): the live status poll, the
//! sidebar footer feed, the activity log overlay and the top-bar "Pando" control with its
//! quick-settings popover.
//!
//! The sparkle button next to it keeps opening the assistant (right panel, Agent tab); this
//! control is only about the connection: status chip, what is degraded while Pando is away,
//! per-graph feature switches, a link to the settings and the activity log.

use crate::views::dims;
use std::time::Duration;

use bitacora_config::{PandoFeature, PandoSettings};
use bitacora_runtime::{ActivityLog, PandoStatus};
use rust_i18n::t;

use super::Workspace;
use crate::ui::switch::Switch;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    Anchor, AnyElement, Context, Disableable as _, FluentBuilder as _, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Task, Window,
    anchored, deferred, div, h_flex, v_flex,
};
use crate::views::kit::{Button, Chip, Overline, PopoverShell};
use crate::views::pando_status::PandoState;
use crate::views::settings::Section;

/// Delay between two reads of the session's Pando status.
const POLL: Duration = Duration::from_secs(3);

/// Everything the workspace knows about Pando.
#[derive(Debug, Default)]
pub(super) struct PandoUi {
    pub(super) settings: PandoSettings,
    pub(super) consented: bool,
    pub(super) live: Option<PandoStatus>,
    /// `(synced, pending)` of the semantic sync, when it runs.
    pub(super) sync: Option<(u64, u64)>,
    pub(super) popover_open: bool,
    poll: Option<Task<()>>,
}

fn feature_key(feature: PandoFeature) -> &'static str {
    match feature {
        PandoFeature::SemanticSearch => "semantic",
        PandoFeature::AgentChat => "chat",
        PandoFeature::McpBridge => "bridge",
        PandoFeature::JournalReview => "review",
        PandoFeature::Recommendations => "recommend",
    }
}

impl Workspace {
    /// The state shown to the user.
    pub fn pando_state(&self) -> PandoState {
        PandoState::derive(
            &self.pando_ui.settings,
            self.pando_ui.consented,
            self.pando_ui.live.as_ref(),
        )
    }

    /// Whether the quick-settings popover is open.
    pub fn pando_popover_open(&self) -> bool {
        self.pando_ui.popover_open
    }

    /// Re-reads `pando.json`, tells the sidebar and makes sure the status poll runs.
    pub(super) fn refresh_pando_ui(&mut self, cx: &mut Context<Self>) {
        let settings = self
            .config
            .pando_settings_path
            .as_deref()
            .and_then(|file| bitacora_runtime::load_pando_settings(file).ok())
            .unwrap_or_default();
        let consented = self
            .graph_root
            .as_deref()
            .is_some_and(|root| settings.has_consent(&root.to_string_lossy()));
        let chosen = settings.chat_model.clone();
        self.chat
            .update(cx, |chat, cx| chat.set_chosen_model(chosen, cx));
        self.pando_ui.settings = settings;
        self.pando_ui.consented = consented;
        if !self.pando_ui.settings.is_active() {
            self.pando_ui.live = None;
            self.pando_ui.sync = None;
        }
        self.push_pando_state(cx);
        self.start_pando_poll(cx);
        cx.notify();
    }

    /// Saves the model picked in the agent panel in `pando.json` (BIT-US-0180).
    pub(super) fn remember_chat_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        let Some(file) = self.config.pando_settings_path.clone() else {
            return;
        };
        let mut settings = bitacora_runtime::load_pando_settings(&file).unwrap_or_default();
        if settings.chat_model == model {
            return;
        }
        settings.chat_model = model;
        if let Err(e) = settings.save(&file) {
            tracing::warn!("cannot remember the chat model: {e}");
            return;
        }
        self.pando_ui.settings = settings;
        cx.notify();
    }

    fn push_pando_state(&mut self, cx: &mut Context<Self>) {
        let state = self.pando_state();
        self.sidebar
            .update(cx, |s, cx| s.set_pando_state(state, cx));
        self.refresh_ai_ui(cx);
    }

    pub(super) fn apply_pando_live(
        &mut self,
        status: PandoStatus,
        sync: Option<(u64, u64)>,
        cx: &mut Context<Self>,
    ) {
        if self.pando_ui.live.as_ref() == Some(&status) && self.pando_ui.sync == sync {
            return;
        }
        self.pando_ui.live = Some(status);
        self.pando_ui.sync = sync;
        // The chat profiles Pando exposes are known once it is connected.
        self.chat
            .update(cx, |chat, cx| chat.refresh_model_choices(cx));
        self.push_pando_state(cx);
        cx.notify();
    }

    fn start_pando_poll(&mut self, cx: &mut Context<Self>) {
        if self.pando_ui.poll.is_some() {
            return;
        }
        self.pando_ui.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let rx = this
                    .update(cx, |this, _| {
                        let active = this.pando_ui.settings.is_active();
                        this.session_handle.clone().filter(|_| active).map(|h| {
                            h.run(|s| {
                                (
                                    s.pando_status(),
                                    s.semantic_status().map(|st| (st.synced, st.pending)),
                                )
                            })
                        })
                    })
                    .ok()
                    .flatten();
                if let Some(rx) = rx
                    && let Ok((status, sync)) = rx.recv().await
                    && this
                        .update(cx, |this, cx| this.apply_pando_live(status, sync, cx))
                        .is_err()
                {
                    return;
                }
                cx.background_executor().timer(POLL).await;
            }
        }));
    }

    pub(super) fn set_pando_popover(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.pando_ui.popover_open != open {
            self.pando_ui.popover_open = open;
            cx.notify();
        }
    }

    /// Opens the Pando page of the settings.
    pub fn open_pando_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_pando_popover(false, cx);
        self.open_settings(Some(Section::Pando), window, cx);
    }

    /// Opens the Pando activity log overlay.
    pub fn open_pando_activity(&mut self, cx: &mut Context<Self>) {
        self.set_pando_popover(false, cx);
        let log = self
            .config
            .pando_settings_path
            .as_deref()
            .map(ActivityLog::beside);
        let handle = self.session_handle.clone();
        self.pando_activity
            .update(cx, |a, cx| a.open_with(log, handle, cx));
    }

    /// Switches a feature for this graph from the popover; saves like the settings page does.
    fn quick_feature(
        &mut self,
        feature: PandoFeature,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_pando_popover(false, cx);
        self.settings.update(cx, |s, cx| {
            s.reload_pando(window, cx);
            s.set_pando_feature(feature, on, cx);
        });
    }

    /// The "Pando" control of the title bar and its popover.
    pub(super) fn pando_control(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.bitacora().clone();
        let colors = theme.colors;
        let metrics = theme.metrics.clone();
        let state = self.pando_state();
        let open = self.pando_ui.popover_open;

        let trigger = h_flex()
            .id("top-pando")
            .debug_selector(|| "top-pando".to_string())
            .h(metrics.icon_button_sm)
            .px(metrics.space[4])
            .gap(metrics.space[3])
            .items_center()
            .rounded(metrics.radius_control)
            .cursor_pointer()
            .when(open, |d| d.bg(colors.hover))
            .hover(move |s| s.bg(colors.hover))
            .text_color(colors.text_2)
            .type_style(&theme.type_scale.caption)
            .child(
                div()
                    .flex_shrink_0()
                    .size(dims::PX_7)
                    .rounded_full()
                    .bg(state.dot(&theme)),
            )
            .child(t!("pando_status.name").to_string())
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.pando_ui.popover_open;
                this.set_pando_popover(open, cx);
            }));

        let popover = open.then(|| self.pando_popover(state, cx));
        div()
            .relative()
            .child(trigger)
            .when_some(popover, |d, p| d.child(p))
            .into_any_element()
    }

    fn pando_popover(&mut self, state: PandoState, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.bitacora().clone();
        let colors = theme.colors;
        let metrics = theme.metrics.clone();
        let live = self.pando_ui.live.clone();
        let in_graph = self.graph_root.is_some();
        let usable = state.is_enabled() && in_graph;

        let mut body = v_flex().gap(metrics.space[4]).child(
            h_flex()
                .gap(metrics.space[4])
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .type_style(&theme.type_scale.ui)
                        .child(t!("pando_status.name").to_string()),
                )
                .child(Chip::new(state.label()).tone(state.tone())),
        );
        body = body.child(
            div()
                .id("pando-popover-detail")
                .debug_selector(|| "pando-popover-detail".to_string())
                .text_color(colors.text_2)
                .type_style(&theme.type_scale.ui_small)
                .child(state.detail(live.as_ref())),
        );
        if let Some(note) = state.degradation() {
            body = body.child(
                div()
                    .id("pando-popover-degraded")
                    .debug_selector(|| "pando-popover-degraded".to_string())
                    .text_color(colors.muted)
                    .type_style(&theme.type_scale.caption)
                    .child(note),
            );
        }

        body = body.child(Overline::new(
            t!("pando_status.popover.graph_features").to_string(),
        ));
        let settings = self.pando_ui.settings.clone();
        let mut features = v_flex().gap(metrics.space[2]);
        for feature in PandoFeature::ALL {
            let k = feature_key(feature);
            let on = settings.features.get(&feature).copied().unwrap_or(true);
            let label_key = format!("settings.pando.feature_{k}");
            let feature_label = t!(&label_key).to_string();
            features = features.child(
                h_flex()
                    .gap(metrics.space[4])
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if usable { colors.text } else { colors.muted })
                            .type_style(&theme.type_scale.ui_small)
                            .child(feature_label),
                    )
                    .child(
                        Switch::new(("pando-quick-feature", feature as usize))
                            .checked(on && state.is_enabled())
                            .disabled(!usable)
                            .on_click(cx.listener(move |this, on: &bool, window, cx| {
                                this.quick_feature(feature, *on, window, cx);
                            })),
                    ),
            );
        }
        body = body.child(features);

        let sync = match self.pando_ui.sync {
            Some((synced, pending)) if state.is_enabled() => t!(
                "pando_status.popover.sync_counts",
                synced = synced,
                pending = pending
            )
            .to_string(),
            _ if state.is_enabled() => t!("pando_status.popover.sync_none").to_string(),
            _ => String::new(),
        };
        if !sync.is_empty() {
            body = body.child(
                div()
                    .text_color(colors.muted)
                    .type_style(&theme.type_scale.caption)
                    .child(sync),
            );
        }

        let settings_label = if state == PandoState::NotConfigured {
            t!("pando_status.popover.set_up")
        } else {
            t!("pando_status.popover.open_settings")
        }
        .to_string();
        body = body.child(
            h_flex()
                .gap(metrics.space[3])
                .child(
                    Button::new("pando-popover-settings")
                        .secondary()
                        .label(settings_label)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_pando_settings(window, cx);
                        })),
                )
                .child(
                    Button::new("pando-popover-activity")
                        .ghost()
                        .label(t!("pando_status.popover.activity").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.open_pando_activity(cx))),
                ),
        );

        let weak = cx.entity().downgrade();
        deferred(
            anchored().anchor(Anchor::TopRight).snap_to_window().child(
                div().mt(metrics.space[2]).child(
                    PopoverShell::new("pando-popover")
                        .width(dims::PX_320)
                        .on_dismiss(move |_, cx| {
                            let _ = weak.update(cx, |this, cx| this.set_pando_popover(false, cx));
                        })
                        .child(body),
                ),
            ),
        )
        .priority(10)
        .into_any_element()
    }
}
