//! AI assistance of the workspace (BIT-US-0151, BIT-US-0152): the journal review card, the
//! suggestion chips, their entry points and the two optional timers.
//!
//! The workspace owns the two entities, feeds them the graph context and the feature gates (from
//! `pando.json`, the graph's consent and the live Pando status) and routes what they ask for:
//! opening a task or a page, and re-arming the daily schedule after a failed run. The schedule
//! ([`DailySchedule`]) and the auto recommender ([`AutoRecommender`]) are pure state machines of
//! the backend; a single slow timer here drives both. Both are off unless the user switched them
//! on in Settings > Pando, and neither runs without a connected Pando and the graph's consent.

use std::time::{Duration, Instant};

use bitacora_runtime::ai::cache::{DailySchedule, ScheduleGate, SystemWallClock};
use bitacora_runtime::ai::recommend::AutoGate;
use bitacora_runtime::ai::{AutoRecommender, ReviewRange};
use rust_i18n::t;

use super::Workspace;
use crate::nav::Route;
use crate::ui::{AppContext as _, Context, Entity, Level, Subscription, Task, Window, notify};
use crate::views::ai_assist::dismiss::DismissStore;
use crate::views::ai_assist::review_card::{ReviewCard, ReviewEvent, today_range, week_range};
use crate::views::ai_assist::suggestions::{SuggestEvent, SuggestionsView};
use crate::views::ai_assist::{AiContext, AiGate};
use bitacora_config::PandoFeature;

/// How often the schedule and the auto mode are looked at.
const TICK: Duration = Duration::from_secs(20);
/// Quiet time on a page before the auto mode recommends for it.
const AUTO_DEBOUNCE: Duration = Duration::from_secs(45);
/// Least time between two automatic recommendation runs.
const AUTO_MIN_GAP: Duration = Duration::from_secs(5 * 60);
/// Wait before a failed scheduled review is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// What the workspace keeps for the AI surfaces.
pub(super) struct AiUi {
    pub(super) review: Entity<ReviewCard>,
    pub(super) suggestions: Entity<SuggestionsView>,
    schedule: DailySchedule,
    auto: AutoRecommender,
    started: Instant,
    /// The review in flight was started by the schedule.
    scheduled_run: bool,
    retry_at: Option<Instant>,
    /// The page the auto mode last saw.
    seen_page: Option<String>,
    timer: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl std::fmt::Debug for AiUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AiUi")
    }
}

impl AiUi {
    pub(super) fn new(cx: &mut Context<Workspace>) -> Self {
        let review = cx.new(|_| ReviewCard::new());
        let suggestions = cx.new(|_| SuggestionsView::new());
        let subs = vec![
            cx.subscribe(&review, |this, _, event: &ReviewEvent, cx| {
                this.on_review_event(event, cx);
            }),
            cx.subscribe(&suggestions, |this, _, event: &SuggestEvent, cx| {
                this.on_suggest_event(event, cx);
            }),
        ];
        Self {
            review,
            suggestions,
            schedule: DailySchedule::disabled(),
            auto: AutoRecommender::new(AUTO_DEBOUNCE, AUTO_MIN_GAP),
            started: Instant::now(),
            scheduled_run: false,
            retry_at: None,
            seen_page: None,
            timer: None,
            _subs: subs,
        }
    }
}

impl Workspace {
    /// The review and recommendation gates from the Pando settings, the graph's consent and the
    /// live status. Also applies the schedule and auto-mode switches.
    pub(super) fn refresh_ai_ui(&mut self, cx: &mut Context<Self>) {
        let p = &self.pando_ui;
        let review = AiGate::derive(
            &p.settings,
            PandoFeature::JournalReview,
            p.consented,
            p.live.as_ref(),
        );
        let recommend = AiGate::derive(
            &p.settings,
            PandoFeature::Recommendations,
            p.consented,
            p.live.as_ref(),
        );
        let ai = p.settings.ai.clone();
        self.ai_ui
            .review
            .update(cx, |card, cx| card.set_gate(review, cx));
        self.ai_ui
            .suggestions
            .update(cx, |chips, cx| chips.set_gate(recommend, cx));
        self.ai_ui.schedule.enabled = ai.review_daily && review.enabled;
        self.ai_ui.schedule.at_minute = ai.review_at_minute;
        self.ai_ui
            .auto
            .set_enabled(ai.recommend_auto && recommend.enabled);
        self.start_ai_timer(cx);
    }

    /// Gives the AI surfaces the open graph (or takes it away when the session is gone).
    pub(super) fn push_ai_context(&mut self, cx: &mut Context<Self>) {
        let ctx = match (
            self.session_handle.clone(),
            self.handle.clone(),
            self.link.clone(),
        ) {
            (Some(session), Some(graph), Some(link)) => Some(AiContext {
                session,
                graph,
                link,
            }),
            _ => None,
        };
        let store = match (&ctx, self.config.state_dir.as_deref(), &self.graph_root) {
            (Some(_), Some(dir), Some(root)) => DismissStore::load(dir, &root.to_string_lossy()),
            _ => DismissStore::memory(),
        };
        self.ai_ui
            .review
            .update(cx, |card, cx| card.set_context(ctx.clone(), cx));
        self.ai_ui.suggestions.update(cx, |chips, cx| {
            chips.set_context(ctx, cx);
            chips.set_dismiss_store(store, cx);
        });
    }

    fn start_ai_timer(&mut self, cx: &mut Context<Self>) {
        if self.ai_ui.timer.is_some() {
            return;
        }
        self.ai_ui.timer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |this, cx| this.ai_tick(cx)).is_err() {
                    return;
                }
            }
        }));
    }

    fn ai_tick(&mut self, cx: &mut Context<Self>) {
        let (connected, consent) = (
            self.pando_ui
                .live
                .as_ref()
                .is_some_and(|s| matches!(s, bitacora_runtime::PandoStatus::Connected { .. })),
            self.pando_ui.consented,
        );
        let now = Instant::now();
        if self.ai_ui.retry_at.is_some_and(|t| now >= t) {
            self.ai_ui.retry_at = None;
            self.ai_ui.schedule.retry();
        }
        let idle = !self.ai_ui.review.read(cx).running();
        if let Some(range) = self.ai_ui.schedule.due(
            &SystemWallClock,
            ScheduleGate {
                connected,
                consent,
                idle,
            },
        ) {
            self.ai_ui.scheduled_run = true;
            self.ai_ui
                .review
                .update(cx, |card, cx| card.request(range, false, cx));
        }
        // A page the user moved to counts as a change: the quiet period starts over.
        let page = self.panel.read(cx).local_page().map(str::to_owned);
        if page != self.ai_ui.seen_page {
            self.ai_ui.seen_page.clone_from(&page);
            self.ai_touch(page, cx);
        }
        if let Some(page) = self.ai_ui.auto.poll(
            self.ai_ui.started.elapsed(),
            AutoGate { connected, consent },
        ) {
            self.ai_ui
                .suggestions
                .update(cx, |chips, cx| chips.request_for(&page, cx));
        }
    }

    fn ai_touch(&mut self, page: Option<String>, _cx: &mut Context<Self>) {
        if let Some(page) = page {
            self.ai_ui.auto.touch(&page, self.ai_ui.started.elapsed());
        }
    }

    /// The page on screen may have changed on disk: restarts the auto mode's quiet period.
    pub(super) fn ai_page_activity(&mut self, cx: &mut Context<Self>) {
        let page = self.panel.read(cx).local_page().map(str::to_owned);
        self.ai_touch(page, cx);
    }

    fn on_review_event(&mut self, event: &ReviewEvent, cx: &mut Context<Self>) {
        match event {
            ReviewEvent::Navigate(target) => self.on_stack_navigate(target, cx),
            ReviewEvent::Finished { ok } => {
                if std::mem::take(&mut self.ai_ui.scheduled_run) && !ok {
                    // Try again later today, not at the next tick.
                    self.ai_ui.retry_at = Some(Instant::now() + RETRY_AFTER);
                }
            }
        }
    }

    fn on_suggest_event(&mut self, event: &SuggestEvent, cx: &mut Context<Self>) {
        match event {
            SuggestEvent::Navigate(target) => self.on_stack_navigate(target, cx),
            SuggestEvent::Finished { .. } => self.ai_ui.auto.finished(),
            // The editors follow the queue, so an accepted edit needs no refresh here.
            SuggestEvent::Edited(_) => {}
        }
    }

    /// "Review today" / "Review this week": shows the journals with the card and starts a run.
    pub(super) fn run_review_command(
        &mut self,
        week: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(reason) = self.ai_ui.review.read(cx).unavailable_reason() {
            notify(
                window,
                cx,
                Level::Info,
                t!("ai.review.cmd_unavailable", reason = reason).to_string(),
            );
            return;
        }
        let Some(today) = (self.clock.0)() else {
            return;
        };
        let range: ReviewRange = if week {
            week_range(today)
        } else {
            today_range(today)
        };
        self.navigate(Route::Journals, cx);
        self.ai_ui
            .review
            .update(cx, |card, cx| card.request(range, false, cx));
    }
}
