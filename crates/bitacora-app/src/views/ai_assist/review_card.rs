//! The journal review card (BIT-US-0151): a summary of one day or one week of journals with the
//! themes, the mood, the tasks still open and the next actions an agent found.
//!
//! The card sits above the journals feed in the amber AI treatment (it is generated text the user
//! has not written). Starting a review is always a user action ("Review today" / "Review this
//! week", also in the command palette) or the optional daily schedule that the workspace drives;
//! the run itself (`bitacora_runtime::ai::run_review`) happens on the tokio runtime. A review
//! never writes to the graph: the cache is machine-local and the tasks listed are only links to
//! blocks that exist. Content hidden by the consent exclusions is dropped by the backend guard.

use bitacora_core::date::Date;
use bitacora_runtime::ai::{ReviewRange, ReviewReport, run_review};
use rust_i18n::t;

use super::{AiContext, AiGate};
use crate::render::inline::NavTarget;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    AnyElement, Context, EventEmitter, FluentBuilder as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    Window, div, h_flex, v_flex,
};
use crate::views::kit::{Button, Card, Chip, ChipTone, Glyph, IconButton, Overline, glyph};

/// What the card asks of its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewEvent {
    /// Open a block (a pending task of the review).
    Navigate(NavTarget),
    /// A run ended; `ok` is false for a failure (the daily schedule re-arms itself then).
    Finished {
        /// The run produced a review.
        ok: bool,
    },
}

/// Where the card stands.
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewState {
    /// Nothing asked yet (the entry points show).
    Idle,
    /// A run is in flight.
    Running {
        /// The days under review.
        range: ReviewRange,
    },
    /// A review is on screen.
    Ready {
        /// The days reviewed.
        range: ReviewRange,
        /// The validated review.
        report: Box<ReviewReport>,
    },
    /// The run failed.
    Failed {
        /// The days asked for.
        range: ReviewRange,
        /// What went wrong, localized or from the backend.
        message: String,
    },
}

/// The seven days ending today.
#[must_use]
pub fn week_range(today: Date) -> ReviewRange {
    let from = today.add_days(-6).unwrap_or(today);
    ReviewRange {
        from: i64::from(from.journal_day()),
        to: i64::from(today.journal_day()),
    }
}

/// Today only.
#[must_use]
pub fn today_range(today: Date) -> ReviewRange {
    ReviewRange::day(i64::from(today.journal_day()))
}

/// `2026-10-07` or `2026-10-01 - 2026-10-07`.
#[must_use]
pub fn range_label(range: &ReviewRange) -> String {
    if range.from == range.to {
        range.from_text()
    } else {
        format!("{} - {}", range.from_text(), range.to_text())
    }
}

/// The review card view.
pub struct ReviewCard {
    gate: AiGate,
    ctx: Option<AiContext>,
    state: ReviewState,
    generation: u64,
    task: Option<Task<()>>,
}

impl std::fmt::Debug for ReviewCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewCard")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<ReviewEvent> for ReviewCard {}

impl Default for ReviewCard {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewCard {
    /// A hidden card (no graph, feature off).
    #[must_use]
    pub fn new() -> Self {
        Self {
            gate: AiGate::default(),
            ctx: None,
            state: ReviewState::Idle,
            generation: 0,
            task: None,
        }
    }

    /// The graph the card reviews (`None` when it is closed: the card resets).
    pub fn set_context(&mut self, ctx: Option<AiContext>, cx: &mut Context<Self>) {
        if ctx.is_none() {
            self.reset();
        }
        self.ctx = ctx;
        cx.notify();
    }

    /// Feature switch, consent and connection. Turning the feature off clears the card, so
    /// nothing generated stays on screen after the user switched it off.
    pub fn set_gate(&mut self, gate: AiGate, cx: &mut Context<Self>) {
        if self.gate == gate {
            return;
        }
        if !gate.enabled {
            self.reset();
        }
        self.gate = gate;
        cx.notify();
    }

    fn reset(&mut self) {
        self.generation += 1;
        self.task = None;
        self.state = ReviewState::Idle;
    }

    /// The card is offered (the feature is on for this graph).
    #[must_use]
    pub fn visible(&self) -> bool {
        self.gate.enabled
    }

    /// Where the card stands.
    #[must_use]
    pub fn state(&self) -> &ReviewState {
        &self.state
    }

    /// Whether a run is in flight.
    #[must_use]
    pub fn running(&self) -> bool {
        matches!(self.state, ReviewState::Running { .. })
    }

    /// Why a review cannot start now, `None` when it can.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<String> {
        if !self.gate.enabled {
            Some(t!("ai.review.off").to_string())
        } else if !self.gate.connected {
            Some(t!("ai.not_connected").to_string())
        } else if self.ctx.is_none() {
            Some(t!("ai.no_graph").to_string())
        } else {
            None
        }
    }

    /// Reviews `range` on the Pando runtime. `force` skips the machine-local cache. Does
    /// nothing while another run is in flight.
    pub fn request(&mut self, range: ReviewRange, force: bool, cx: &mut Context<Self>) {
        if self.running() {
            return;
        }
        if let Some(message) = self.unavailable_reason() {
            self.state = ReviewState::Failed { range, message };
            cx.emit(ReviewEvent::Finished { ok: false });
            cx.notify();
            return;
        }
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.state = ReviewState::Running { range };
        let deps = ctx.session.run(|s| s.review_deps());
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome: Result<ReviewReport, String> = match deps.recv().await {
                Ok(Ok(deps)) => {
                    let task = this.update(cx, |_, cx| {
                        crate::tokio_bridge::spawn(cx, async move {
                            run_review(&deps, range, force).await
                        })
                    });
                    match task {
                        Ok(task) => match task.await {
                            Ok(result) => result.map_err(|e| e.to_string()),
                            Err(e) => Err(e.to_string()),
                        },
                        Err(_) => return,
                    }
                }
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err(t!("ai.no_graph").to_string()),
            };
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.finish(range, outcome, cx);
                }
            });
        }));
        cx.notify();
    }

    /// Shows the end of a run. Public so the result path can be exercised without Pando.
    pub fn finish(
        &mut self,
        range: ReviewRange,
        outcome: Result<ReviewReport, String>,
        cx: &mut Context<Self>,
    ) {
        self.task = None;
        let ok = outcome.is_ok();
        self.state = match outcome {
            Ok(report) => ReviewState::Ready {
                range,
                report: Box::new(report),
            },
            Err(message) => ReviewState::Failed { range, message },
        };
        cx.emit(ReviewEvent::Finished { ok });
        cx.notify();
    }

    /// Stops the run in flight (its result is dropped) and goes back to the entry points.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.reset();
        cx.notify();
    }

    /// Leaves a shown review or error.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.running() {
            self.state = ReviewState::Idle;
            cx.notify();
        }
    }

    fn today(&self) -> Option<Date> {
        crate::data::today_local()
    }

    fn start(&mut self, week: bool, cx: &mut Context<Self>) {
        if let Some(today) = self.today() {
            let range = if week {
                week_range(today)
            } else {
                today_range(today)
            };
            self.request(range, false, cx);
        }
    }

    fn note(&self, text: impl Into<SharedString>, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora();
        div()
            .text_color(bt.colors.text_2)
            .type_style(&bt.type_scale.ui_small)
            .child(text.into())
            .into_any_element()
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora().clone();
        h_flex()
            .gap(bt.metrics.space[3])
            .items_center()
            .child(glyph(Glyph::Sparkle, bt.metrics.icon_sm, bt.colors.ai, cx))
            .child(
                div()
                    .flex_1()
                    .child(Overline::new(t!("ai.review.title").to_string())),
            )
            .into_any_element()
    }

    fn entry_points(&self, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora().clone();
        let usable = self.unavailable_reason().is_none();
        let mut col = v_flex()
            .gap(bt.metrics.space[4])
            .child(self.note(t!("ai.review.hint").to_string(), cx));
        col = col.child(
            h_flex()
                .gap(bt.metrics.space[3])
                .child(
                    Button::new("review-today")
                        .label(t!("ai.review.today").to_string())
                        .ai()
                        .compact()
                        .disabled(!usable)
                        .on_click(cx.listener(|this, _, _, cx| this.start(false, cx))),
                )
                .child(
                    Button::new("review-week")
                        .label(t!("ai.review.week").to_string())
                        .ai()
                        .compact()
                        .disabled(!usable)
                        .on_click(cx.listener(|this, _, _, cx| this.start(true, cx))),
                ),
        );
        if let Some(why) = self.unavailable_reason().filter(|_| !usable) {
            col = col.child(
                div()
                    .text_color(bt.colors.muted)
                    .type_style(&bt.type_scale.caption)
                    .child(why),
            );
        }
        col.into_any_element()
    }

    fn ready(
        &self,
        range: &ReviewRange,
        report: &ReviewReport,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let review = &report.review;
        let mut col = v_flex().gap(m.space[4]).child(
            h_flex()
                .gap(m.space[3])
                .items_center()
                .child(
                    div()
                        .text_color(bt.colors.muted)
                        .type_style(&bt.type_scale.mono)
                        .child(range_label(range)),
                )
                .when_some(review.mood.clone(), |row, mood| {
                    row.child(Chip::new(mood).tone(ChipTone::Outline))
                })
                .when(report.from_cache, |row| {
                    row.child(
                        div()
                            .text_color(bt.colors.muted)
                            .type_style(&bt.type_scale.caption)
                            .child(t!("ai.review.cached").to_string()),
                    )
                }),
        );
        col = col.child(
            div()
                .id("review-summary")
                .text_color(bt.colors.text)
                .type_style(&bt.type_scale.ui)
                .child(SharedString::from(review.summary.clone())),
        );
        if !review.themes.is_empty() {
            let mut themes = h_flex().flex_wrap().gap(m.space[2]);
            for theme in &review.themes {
                themes = themes.child(Chip::new(theme.clone()).tone(ChipTone::Ai));
            }
            col = col.child(themes);
        }
        if !review.pending_tasks.is_empty() {
            let mut list = v_flex().id("review-tasks").gap(m.space[2]).child(
                Overline::new(t!("ai.review.tasks").to_string()).count(review.pending_tasks.len()),
            );
            for (ix, task) in review.pending_tasks.iter().enumerate() {
                let uuid = task.block_uuid.clone();
                list = list.child(
                    h_flex()
                        .id(("review-task", ix))
                        .gap(m.space[3])
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(ReviewEvent::Navigate(NavTarget::Block(uuid.clone())));
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(bt.colors.text)
                                .type_style(&bt.type_scale.ui_small)
                                .child(SharedString::from(task.text.clone())),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(bt.colors.muted)
                                .type_style(&bt.type_scale.caption)
                                .child(SharedString::from(task.page.clone())),
                        ),
                );
            }
            col = col.child(list);
        }
        if !review.next_actions.is_empty() {
            let mut list = v_flex()
                .id("review-actions")
                .gap(m.space[2])
                .child(Overline::new(t!("ai.review.actions").to_string()));
            for action in &review.next_actions {
                list = list.child(
                    h_flex()
                        .gap(m.space[3])
                        .child(glyph(Glyph::ChevronRight, m.icon_sm, bt.colors.ai, cx))
                        .child(
                            div()
                                .flex_1()
                                .text_color(bt.colors.text)
                                .type_style(&bt.type_scale.ui_small)
                                .child(SharedString::from(action.clone())),
                        ),
                );
            }
            col = col.child(list);
        }
        let range = *range;
        col.child(
            h_flex()
                .gap(m.space[3])
                .child(
                    Button::new("review-refresh")
                        .label(t!("ai.review.refresh").to_string())
                        .ghost()
                        .compact()
                        .disabled(self.unavailable_reason().is_some())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state = ReviewState::Idle;
                            this.request(range, true, cx);
                        })),
                )
                .child(
                    Button::new("review-close")
                        .label(t!("ai.close").to_string())
                        .ghost()
                        .compact()
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                ),
        )
        .into_any_element()
    }
}

impl Render for ReviewCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible() {
            return div().into_any_element();
        }
        let bt = cx.bitacora().clone();
        let body: AnyElement = match self.state.clone() {
            ReviewState::Idle => self.entry_points(cx),
            ReviewState::Running { range } => v_flex()
                .gap(bt.metrics.space[3])
                .child(self.note(
                    t!("ai.review.running", range = range_label(&range)).to_string(),
                    cx,
                ))
                .child(
                    Button::new("review-cancel")
                        .label(t!("ai.cancel").to_string())
                        .ghost()
                        .compact()
                        .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                )
                .into_any_element(),
            ReviewState::Ready { range, report } => self.ready(&range, &report, cx),
            ReviewState::Failed { range, message } => v_flex()
                .gap(bt.metrics.space[3])
                .child(
                    div()
                        .id("review-error")
                        .text_color(bt.colors.warn)
                        .type_style(&bt.type_scale.ui_small)
                        .child(SharedString::from(message)),
                )
                .child(
                    h_flex()
                        .gap(bt.metrics.space[3])
                        .child(
                            Button::new("review-retry")
                                .label(t!("ai.retry").to_string())
                                .ai()
                                .compact()
                                .disabled(self.unavailable_reason().is_some())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.state = ReviewState::Idle;
                                    this.request(range, false, cx);
                                })),
                        )
                        .child(
                            IconButton::new("review-dismiss", Glyph::Close)
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        ),
                )
                .into_any_element(),
        };
        div()
            .id("journal-review-card")
            .flex_shrink_0()
            .max_h(crate::ui::px(340.))
            .overflow_y_scroll()
            .px(bt.metrics.space[6])
            .pt(bt.metrics.space[5])
            .child(
                Card::new().ai().child(
                    v_flex()
                        .gap(bt.metrics.space[4])
                        .child(self.header(cx))
                        .child(body),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use bitacora_runtime::ai::Review;
    use bitacora_runtime::ai::review::PendingTask;

    fn report(from_cache: bool) -> ReviewReport {
        ReviewReport {
            review: Review {
                summary: "A busy day".into(),
                themes: vec!["planning".into()],
                mood: Some("focused".into()),
                pending_tasks: vec![PendingTask {
                    block_uuid: "11111111-1111-4111-8111-111111111111".into(),
                    text: "TODO write report".into(),
                    page: "Oct 7th, 2026".into(),
                }],
                next_actions: vec!["Send the report".into()],
            },
            from_cache,
            dropped_tasks: Vec::new(),
        }
    }

    #[test]
    fn ranges_cover_today_and_the_last_seven_days() {
        let today = Date::new(2026, 10, 7).unwrap();
        assert_eq!(today_range(today), ReviewRange::day(20_261_007));
        let w = week_range(today);
        assert_eq!((w.from, w.to), (20_261_001, 20_261_007));
        // Across a month boundary.
        let w = week_range(Date::new(2026, 3, 3).unwrap());
        assert_eq!((w.from, w.to), (20_260_225, 20_260_303));
        assert_eq!(range_label(&ReviewRange::day(20_261_007)), "2026-10-07");
        assert_eq!(range_label(&w), "2026-02-25 - 2026-03-03");
    }

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::theme::install(cx, crate::settings::AppSettings::default(), None);
        });
    }

    #[gpui_test]
    fn the_card_follows_the_gate_and_clears_when_switched_off(cx: &mut TestAppContext) {
        setup(cx);
        let (card, cx) = cx.add_window_view(|_, _| ReviewCard::new());
        card.update(cx, |c, _| assert!(!c.visible()));
        let range = ReviewRange::day(20_261_007);
        card.update(cx, |c, cx| {
            c.set_gate(
                AiGate {
                    enabled: true,
                    connected: false,
                },
                cx,
            );
            assert!(c.visible());
            // Not connected: a request is refused with a reason instead of running.
            c.request(range, false, cx);
            assert!(matches!(c.state(), ReviewState::Failed { .. }));
            c.finish(range, Ok(report(false)), cx);
            assert!(matches!(c.state(), ReviewState::Ready { .. }));
            c.set_gate(AiGate::default(), cx);
            assert_eq!(c.state(), &ReviewState::Idle);
        });
    }

    #[gpui_test]
    fn a_finished_run_shows_the_review_and_reports_the_outcome(cx: &mut TestAppContext) {
        setup(cx);
        let (card, cx) = cx.add_window_view(|_, _| ReviewCard::new());
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&card, move |_, e: &ReviewEvent, _| {
                sink.borrow_mut().push(e.clone())
            })
            .detach();
        });
        let range = ReviewRange::day(20_261_007);
        card.update(cx, |c, cx| {
            c.set_gate(
                AiGate {
                    enabled: true,
                    connected: true,
                },
                cx,
            );
            c.finish(range, Ok(report(true)), cx);
            c.finish(range, Err("boom".into()), cx);
        });
        assert_eq!(
            *events.borrow(),
            vec![
                ReviewEvent::Finished { ok: true },
                ReviewEvent::Finished { ok: false }
            ]
        );
        card.update(cx, |c, cx| {
            assert!(matches!(c.state(), ReviewState::Failed { message, .. } if message == "boom"));
            c.close(cx);
            assert_eq!(c.state(), &ReviewState::Idle);
        });
    }
}
