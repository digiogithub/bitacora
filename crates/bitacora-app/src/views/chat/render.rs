//! Elements of the chat view: header, transcript, tool-call and approval cards, thread list and
//! composer. Colours, sizes and type come from the theme tokens; amber (`ai`) marks AI-only
//! content: the sparkle, pending proposals, the activity line and AI accents.

use crate::views::dims;
use std::collections::HashMap;
use std::rc::Rc;

use bitacora_runtime::ai::{
    AgentState, ApprovalCard, CardKind, CardState, CardView, ChatMessage, DenyReason, ToolCallView,
};
use rust_i18n::t;

use super::diff::{LineKind, op_diffs, totals};
use super::markdown::{MdBlock, parse_blocks};
use super::tools::{
    CallStatus, call_status, detail_text, duration_text, summarize_args, tokens_text,
    usage_fraction,
};
use super::{ChatView, ChatViewEvent, ContextKind, EditNote, Phase};
use crate::data::IndexResolver;
use crate::nav::OpenIn;
use crate::render::inline::{BlockResolver, NoBlocks, TextLayout, layout_lines};
use crate::ui::input::Textarea;
use crate::ui::text_edit::relative;
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, TypeStyleExt as _};
use crate::ui::{
    ActiveTheme as _, AnyElement, App, Context, FluentBuilder as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, h_flex, px, v_flex,
};
use crate::views::block_view::{Nav, text_element_owned};
use crate::views::kit::{Button, Chip, ChipTone, Glyph, IconButton, Overline, glyph};

/// The tail corner of a user bubble (design doc: radii 14 / 14 / 4 / 14).
const BUBBLE_TAIL: f32 = 4.0;
/// Share of the width a user bubble may take.
const BUBBLE_MAX: f32 = 0.88;
/// Send button side (design doc).
const SEND_SIDE: f32 = 34.0;
/// List marker of a bullet block (a symbol, not a visible word).
const BULLET: &str = "\u{2022}";
/// Diff lines shown per op before folding the rest.
const DIFF_LINES: usize = 40;

/// One rendered block of an answer.
#[derive(Debug, Clone)]
pub enum Rendered {
    Heading(u8, TextLayout),
    Para(TextLayout),
    Bullet(usize, TextLayout),
    Quote(TextLayout),
    Code(String, String),
}

/// Rendered answers by message id; an entry is reused while the text keeps its length (the
/// streamed text only grows), so a delta recomputes one message and the rest cost nothing.
#[derive(Default)]
pub struct LayoutCache(HashMap<String, (usize, Rc<Vec<Rendered>>)>);

impl LayoutCache {
    /// Forgets every entry.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    fn get(&mut self, id: &str, text: &str, resolver: &dyn BlockResolver) -> Rc<Vec<Rendered>> {
        if let Some((len, blocks)) = self.0.get(id)
            && *len == text.len()
        {
            return Rc::clone(blocks);
        }
        let blocks = Rc::new(render_blocks(text, resolver));
        self.0
            .insert(id.to_owned(), (text.len(), Rc::clone(&blocks)));
        blocks
    }
}

/// Parses `text` and lays its inline constructs out (`[[Page]]` and `((block))` become links).
pub fn render_blocks(text: &str, resolver: &dyn BlockResolver) -> Vec<Rendered> {
    parse_blocks(text)
        .into_iter()
        .map(|block| match block {
            MdBlock::Heading { level, text } => {
                Rendered::Heading(level, layout_lines([text.as_str()], resolver))
            }
            MdBlock::Paragraph(text) => Rendered::Para(layout_lines(text.split('\n'), resolver)),
            MdBlock::Bullet { depth, text } => {
                Rendered::Bullet(depth, layout_lines([text.as_str()], resolver))
            }
            MdBlock::Quote(text) => Rendered::Quote(layout_lines(text.split('\n'), resolver)),
            MdBlock::Code { lang, text } => Rendered::Code(lang, text),
        })
        .collect()
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(text) = self.restore_text.take() {
            self.composer
                .update(cx, |c, cx| c.set_value(text, window, cx));
        }
        let bt = cx.bitacora().clone();
        // Follow the stream while the user has not scrolled away from the bottom.
        let max = self.scroll.max_offset().y;
        let at_bottom = max <= px(0.) || self.scroll.offset().y <= -max + dims::PX_4;
        if at_bottom {
            self.follow = true;
        }
        if self.follow {
            self.scroll.scroll_to_bottom();
        }
        let body = if self.threads.open {
            self.thread_list(&bt, cx)
        } else {
            self.transcript(&bt, cx)
        };
        v_flex()
            .id("chat-view")
            .key_context("ChatView")
            .size_full()
            .child(self.header(&bt, cx))
            .child(div().flex_1().min_h_0().child(body))
            .child(self.composer_box(&bt, cx))
    }
}

impl ChatView {
    fn nav(cx: &mut Context<Self>) -> Nav {
        let view = cx.entity();
        Rc::new(move |target, _: OpenIn, cx: &mut App| {
            view.update(cx, |_, cx| cx.emit(ChatViewEvent::Navigate(target)));
        })
    }

    // ---- header (BIT-T-0456) ----------------------------------------------------------------------

    fn header(&self, bt: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let state = self.model.state.clone().unwrap_or_default();
        let mut row = h_flex()
            .id("chat-header")
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(m.space[3])
            .px(m.space[4])
            .py(m.space[3])
            .border_b_1()
            .border_color(c.line)
            .child(glyph(Glyph::Sparkle, m.icon_sm, c.ai, cx))
            .child(
                Chip::new(
                    state
                        .model
                        .clone()
                        .unwrap_or_else(|| t!("chat.model_unknown").to_string()),
                )
                .mono(true),
            );
        if let Some(usage) = usage_el(&state, bt) {
            row = row.child(usage);
        }
        let running = state.running_sub_agents();
        if running > 0 {
            row = row.child(
                Chip::new(t!("chat.sub_agents", count = running).to_string())
                    .tone(ChipTone::Ai)
                    .mono(true),
            );
        }
        row = row
            .child(div().flex_1())
            .child(
                Button::new("chat-threads")
                    .label(t!("chat.threads").to_string())
                    .ghost()
                    .compact()
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_threads(cx))),
            )
            .child(
                IconButton::new("chat-new", Glyph::Plus)
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.new_thread(cx))),
            );
        let activity = self.model.activity.clone().or_else(|| {
            (self.model.running && self.model.activity.is_none())
                .then(|| t!("chat.working").to_string())
        });
        v_flex()
            .flex_none()
            .child(row)
            .when_some(activity, |col, text| {
                col.child(
                    h_flex()
                        .id("chat-activity")
                        .gap(m.space[3])
                        .px(m.space[4])
                        .py(m.space[2])
                        .items_center()
                        .text_color(c.ai)
                        .type_style(&bt.type_scale.mono)
                        .child(div().size(dims::PX_6).rounded(m.radius_pill).bg(c.ai))
                        .child(div().truncate().child(text)),
                )
            })
            .into_any_element()
    }

    // ---- transcript -------------------------------------------------------------------------------

    fn transcript(&mut self, bt: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let m = &bt.metrics;
        let theme = cx.theme().clone();
        let nav = Self::nav(cx);
        let mut layouts = std::mem::take(&mut self.layouts);
        let graph = self.graph.clone();
        let resolver_graph = graph.as_ref().map(|g| IndexResolver(&g.reader));
        let resolver: &dyn BlockResolver = match &resolver_graph {
            Some(r) => r,
            None => &NoBlocks,
        };

        let mut col = v_flex()
            .id("chat-transcript")
            .gap(m.space[5])
            .px(m.space[4])
            .py(m.space[4]);
        let mut shown_cards: Vec<String> = Vec::new();
        let messages = self.model.messages.clone();
        for (ix, msg) in messages.iter().enumerate() {
            col = col.child(self.message_el(
                ix,
                msg,
                &mut layouts,
                resolver,
                bt,
                &theme,
                &nav,
                &mut shown_cards,
                cx,
            ));
        }
        for view in self.model.cards.clone() {
            if !shown_cards.contains(&view.card.id) {
                col = col.child(self.approval_card(&view, bt, cx));
            }
        }
        if let Some(error) = self.model.error.clone() {
            col = col.child(banner(error, bt, true));
        }
        self.layouts = layouts;

        if self.model.messages.is_empty() && self.model.cards.is_empty() {
            return self.empty_state(bt, cx);
        }
        div()
            .id("chat-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(
                |this, event: &crate::ui::canvas::ScrollWheelEvent, _, _| {
                    // Scrolling up leaves the stream; reaching the bottom again rejoins it.
                    if event.delta.pixel_delta(dims::PX_16).y > px(0.) {
                        this.follow = false;
                    }
                },
            ))
            .child(col)
            .into_any_element()
    }

    fn empty_state(&self, bt: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let mut col = v_flex()
            .id("chat-empty")
            .size_full()
            .items_center()
            .justify_center()
            .gap(m.space[3])
            .p(m.space[6])
            .text_center()
            .child(glyph(Glyph::Sparkle, m.icon, c.ai, cx))
            .child(
                div()
                    .text_color(c.text)
                    .type_style(&bt.type_scale.ui)
                    .child(t!("right.agent_empty_title").to_string()),
            )
            .child(
                div()
                    .text_color(c.muted)
                    .type_style(&bt.type_scale.ui_small)
                    .child(t!("chat.empty_hint").to_string()),
            );
        if let Phase::Unavailable(reason) = &self.phase {
            col = col.child(banner(
                t!("chat.unavailable", reason = reason).to_string(),
                bt,
                true,
            ));
        }
        col.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn message_el(
        &self,
        ix: usize,
        msg: &ChatMessage,
        layouts: &mut LayoutCache,
        resolver: &dyn BlockResolver,
        bt: &BitacoraTheme,
        theme: &crate::ui::theme::Theme,
        nav: &Nav,
        shown_cards: &mut Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        if msg.role == "user" {
            let blocks = layouts.get(&msg.id, &msg.text, resolver);
            let labels = self.sent_context.get(&ix).cloned().unwrap_or_default();
            return h_flex()
                .id(("chat-user", ix))
                .w_full()
                .justify_end()
                .child(
                    v_flex()
                        .max_w(relative(BUBBLE_MAX))
                        .gap(m.space[2])
                        .items_end()
                        .child(
                            div()
                                .bg(c.hover)
                                .px(m.space[4])
                                .py(m.space[3])
                                .rounded_tl(m.radius_popover)
                                .rounded_tr(m.radius_popover)
                                .rounded_bl(m.radius_popover)
                                .rounded_br(px(BUBBLE_TAIL))
                                .text_color(c.text)
                                .type_style(&bt.type_scale.panel_body)
                                .child(md_el(ix, &blocks, bt, theme, nav)),
                        )
                        .when(!labels.is_empty(), |d| {
                            d.child(
                                h_flex().flex_wrap().gap(m.space[2]).children(
                                    labels.into_iter().map(|l| {
                                        Chip::new(l).tone(ChipTone::Accent).icon(Glyph::File)
                                    }),
                                ),
                            )
                        }),
                )
                .into_any_element();
        }
        let blocks = layouts.get(&msg.id, &msg.text, resolver);
        let mut col = v_flex().id(("chat-assistant", ix)).gap(m.space[3]);
        if !msg.reasoning.is_empty() {
            col = col.child(self.reasoning_row(msg, bt, cx));
        }
        if !msg.text.is_empty() {
            col = col.child(
                div()
                    .text_color(c.text)
                    .type_style(&bt.type_scale.panel_body)
                    .child(md_el(ix, &blocks, bt, theme, nav)),
            );
        }
        for call in &msg.tool_calls {
            if let Some(view) = self.model.cards.iter().find(|v| v.card.id == call.id) {
                shown_cards.push(call.id.clone());
                col = col.child(self.approval_card(view, bt, cx));
            } else {
                col = col.child(self.tool_card(call, bt, cx));
            }
        }
        if msg.cancelled {
            col = col.child(
                div()
                    .text_color(c.muted)
                    .type_style(&bt.type_scale.caption)
                    .child(t!("chat.stopped").to_string()),
            );
        }
        col.into_any_element()
    }

    fn reasoning_row(
        &self,
        msg: &ChatMessage,
        bt: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let key = format!("{}:reasoning", msg.id);
        let open = self.expanded.contains(&key);
        let toggle = key.clone();
        v_flex()
            .id(SharedString::from(format!("reasoning-{}", msg.id)))
            .gap(m.space[2])
            .child(
                h_flex()
                    .id(SharedString::from(format!("reasoning-head-{}", msg.id)))
                    .gap(m.space[2])
                    .items_center()
                    .cursor_pointer()
                    .text_color(c.muted)
                    .type_style(&bt.type_scale.caption)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_expanded(&toggle, cx)))
                    .child(glyph(
                        if open {
                            Glyph::ChevronDown
                        } else {
                            Glyph::ChevronRight
                        },
                        m.icon_sm,
                        c.muted,
                        cx,
                    ))
                    .child(t!("chat.reasoning").to_string()),
            )
            .when(open, |d| {
                d.child(
                    div()
                        .pl(m.space[5])
                        .text_color(c.muted)
                        .type_style(&bt.type_scale.panel_body)
                        .italic()
                        .child(msg.reasoning.clone()),
                )
            })
            .into_any_element()
    }

    // ---- tool-call cards (BIT-T-0455) -------------------------------------------------------------

    fn tool_card(
        &self,
        call: &ToolCallView,
        bt: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let status = call_status(call);
        let open = self.expanded.contains(&call.id);
        let key = call.id.clone();
        let (icon, tint) = match status {
            CallStatus::Running => (None, c.ai),
            CallStatus::Done => (Some(Glyph::Check), c.ok),
            CallStatus::Failed => (Some(Glyph::CircleAlert), c.warn),
        };
        let summary = summarize_args(&call.args);
        let head = h_flex()
            .id(SharedString::from(format!("tool-head-{}", call.id)))
            .gap(m.space[3])
            .items_center()
            .px(m.space[3])
            .py(m.space[2])
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_expanded(&key, cx)))
            .child(match icon {
                Some(g) => glyph(g, m.icon_sm, tint, cx).into_any_element(),
                None => div()
                    .size(dims::PX_8)
                    .flex_none()
                    .rounded(m.radius_pill)
                    .bg(tint)
                    .into_any_element(),
            })
            .child(
                div()
                    .flex_none()
                    .text_color(c.text)
                    .child(call.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_right()
                    .text_color(c.muted)
                    .child(summary),
            )
            .when_some(call.duration, |d, dur| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(c.muted)
                        .child(duration_text(dur)),
                )
            })
            .child(glyph(
                if open {
                    Glyph::ChevronDown
                } else {
                    Glyph::ChevronRight
                },
                m.icon_sm,
                c.muted,
                cx,
            ));
        v_flex()
            .id(SharedString::from(format!("tool-{}", call.id)))
            .rounded(m.radius_control)
            .border_1()
            .border_color(c.line)
            .type_style(&bt.type_scale.mono)
            .child(head)
            .when(open, |d| {
                d.child(
                    v_flex()
                        .gap(m.space[3])
                        .px(m.space[3])
                        .pb(m.space[3])
                        .child(code_box(
                            t!("chat.tool_input").to_string(),
                            detail_text(&call.args),
                            bt,
                        ))
                        .when_some(call.result.clone(), |d, result| {
                            d.child(code_box(
                                t!("chat.tool_output").to_string(),
                                detail_text(&result),
                                bt,
                            ))
                        }),
                )
            })
            .into_any_element()
    }

    // ---- approval cards (BIT-T-0458) --------------------------------------------------------------

    fn approval_card(
        &self,
        view: &CardView,
        bt: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let card = &view.card;
        let mut body = v_flex()
            .id(SharedString::from(format!("card-{}", card.id)))
            .gap(m.space[3])
            .p(m.space[4])
            .rounded(m.radius_card)
            .border_1()
            .border_color(c.ai_line)
            .bg(c.ai_bg);
        body = match &card.kind {
            CardKind::Edit(preview) => body.child(self.edit_body(preview, bt, cx)),
            CardKind::Permission(req) => {
                let mut b = body
                    .child(
                        h_flex()
                            .gap(m.space[3])
                            .items_center()
                            .child(glyph(Glyph::Sparkle, m.icon_sm, c.ai, cx))
                            .child(
                                div()
                                    .text_color(c.text)
                                    .type_style(&bt.type_scale.ui)
                                    .child(t!("chat.perm_title").to_string()),
                            )
                            .child(Chip::new(req.tool_name.clone()).mono(true)),
                    )
                    .when(!req.description.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(c.text_2)
                                .type_style(&bt.type_scale.panel_body)
                                .child(req.description.clone()),
                        )
                    });
                if !req.action.is_empty() || !req.path.is_empty() {
                    b = b.child(
                        div()
                            .text_color(c.muted)
                            .type_style(&bt.type_scale.mono)
                            .child(format!("{} {}", req.action, req.path).trim().to_owned()),
                    );
                }
                if req.require_explicit_approval {
                    b = b.child(
                        div()
                            .text_color(c.warn)
                            .type_style(&bt.type_scale.caption)
                            .child(t!("chat.perm_explicit").to_string()),
                    );
                }
                b
            }
            CardKind::Question(req) => body.child(self.question_body(card, req, bt, cx)),
        };
        match &view.state {
            CardState::Pending => {
                if !matches!(card.kind, CardKind::Question(_)) {
                    let (approve_id, deny_id) = (card.id.clone(), card.id.clone());
                    if let Some(tool) = card.remember_tool.clone() {
                        let remember_id = card.id.clone();
                        let on = self.is_remembering(&card.id);
                        body = body.child(
                            Button::new(SharedString::from(format!("remember-{}", card.id)))
                                .label(format!(
                                    "{} {} ({tool})",
                                    if on { "[x]" } else { "[ ]" },
                                    t!("chat.remember")
                                ))
                                .ghost()
                                .compact()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_remember(&remember_id, cx);
                                })),
                        );
                    }
                    body = body.child(
                        h_flex()
                            .gap(m.space[3])
                            .items_center()
                            .child(
                                Button::new(SharedString::from(format!("approve-{}", card.id)))
                                    .label(match card.kind {
                                        CardKind::Edit(_) => t!("chat.apply").to_string(),
                                        _ => t!("chat.allow").to_string(),
                                    })
                                    .ai()
                                    .compact()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.approve(&approve_id, cx);
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("deny-{}", card.id)))
                                    .label(t!("chat.deny").to_string())
                                    .secondary()
                                    .compact()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.deny(&deny_id, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_right()
                                    .text_color(c.muted)
                                    .type_style(&bt.type_scale.caption)
                                    .child(
                                        t!("chat.auto_deny", seconds = card.timeout_secs)
                                            .to_string(),
                                    ),
                            ),
                    );
                }
            }
            CardState::Approved => {
                let (text, tint) = match self.edit_notes.get(&card.id) {
                    Some(EditNote::Applied { page, blocks }) => (
                        t!("chat.edit_applied", count = *blocks, page = page).to_string(),
                        c.text,
                    ),
                    Some(EditNote::Failed(why)) => {
                        (t!("chat.edit_failed", error = why).to_string(), c.warn)
                    }
                    None if matches!(card.kind, CardKind::Edit(_)) => {
                        (t!("chat.edit_applying").to_string(), c.muted)
                    }
                    None => (t!("chat.answered").to_string(), c.muted),
                };
                body = body.child(
                    div()
                        .text_color(tint)
                        .type_style(&bt.type_scale.caption)
                        .child(text),
                );
            }
            CardState::Denied(reason) => {
                body = body.child(
                    div()
                        .text_color(c.muted)
                        .type_style(&bt.type_scale.caption)
                        .child(denied_text(*reason)),
                );
            }
        }
        body.into_any_element()
    }

    fn edit_body(
        &self,
        preview: &bitacora_runtime::ai::Preview,
        bt: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let diffs = op_diffs(preview);
        let (added, removed) = totals(&diffs);
        let mut col = v_flex().gap(m.space[3]).child(
            h_flex()
                .gap(m.space[3])
                .items_center()
                .flex_wrap()
                .child(glyph(Glyph::Sparkle, m.icon_sm, c.ai, cx))
                .child(
                    div()
                        .text_color(c.text)
                        .type_style(&bt.type_scale.ui)
                        .child(preview.title.clone()),
                )
                .child(Chip::new(format!("[[{}]]", preview.page)).tone(ChipTone::Accent))
                .child(
                    div()
                        .text_color(c.muted)
                        .type_style(&bt.type_scale.mono)
                        .child(format!("+{added} -{removed}")),
                ),
        );
        for (ix, diff) in diffs.iter().enumerate() {
            let mut block = v_flex()
                .id(("diff", ix))
                .gap(dims::PX_1)
                .child(Overline::new(op_label(diff.kind)));
            for line in diff.lines.iter().take(DIFF_LINES) {
                let (prefix, fg, bg) = match line.kind {
                    LineKind::Added => ("+", c.ai, Some(c.ai_bg)),
                    LineKind::Removed => ("-", c.warn, Some(c.warn.opacity(0.1))),
                    LineKind::Same => (" ", c.muted, None),
                };
                block = block.child(
                    h_flex()
                        .gap(m.space[2])
                        .px(m.space[2])
                        .rounded(m.radius_chip)
                        .text_color(fg)
                        .type_style(&bt.type_scale.mono)
                        .when_some(bg, |d, bg| d.bg(bg))
                        .child(div().flex_none().child(prefix))
                        .child(div().min_w_0().child(line.text.clone())),
                );
            }
            if diff.lines.len() > DIFF_LINES {
                block = block.child(
                    div()
                        .text_color(c.muted)
                        .type_style(&bt.type_scale.caption)
                        .child(
                            t!("chat.diff_more", count = diff.lines.len() - DIFF_LINES).to_string(),
                        ),
                );
            }
            col = col.child(block);
        }
        col.into_any_element()
    }

    fn question_body(
        &self,
        card: &ApprovalCard,
        req: &bitacora_runtime::ai::QuestionRequest,
        bt: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let total = req.questions.len();
        let mut body = v_flex().gap(m.space[3]).child(
            h_flex()
                .gap(m.space[3])
                .items_center()
                .child(glyph(Glyph::Sparkle, m.icon_sm, c.ai, cx))
                .child(
                    div()
                        .text_color(c.text)
                        .type_style(&bt.type_scale.ui)
                        .child(t!("chat.question_title").to_string()),
                ),
        );
        for (qi, q) in req.questions.iter().enumerate() {
            let header = if q.header.is_empty() {
                q.question.clone()
            } else {
                q.header.clone()
            };
            let chosen = self
                .picks
                .get(&card.id)
                .and_then(|p| p.get(qi))
                .and_then(Clone::clone)
                .map(|(_, label)| label);
            let mut options = h_flex().flex_wrap().gap(m.space[2]);
            for (oi, opt) in q.options.iter().enumerate() {
                let (id, header, label) = (card.id.clone(), header.clone(), opt.label.clone());
                let selected = chosen.as_deref() == Some(opt.label.as_str());
                let button = Button::new(SharedString::from(format!("q-{}-{qi}-{oi}", card.id)))
                    .label(opt.label.clone())
                    .compact()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick(&id, qi, &header, &label, total, cx);
                    }));
                options = options.child(if selected {
                    button.ai()
                } else {
                    button.secondary()
                });
            }
            body = body
                .child(
                    div()
                        .text_color(c.text_2)
                        .type_style(&bt.type_scale.panel_body)
                        .child(q.question.clone()),
                )
                .child(options);
        }
        let id = card.id.clone();
        body.child(
            Button::new(SharedString::from(format!("skip-{}", card.id)))
                .label(t!("chat.skip").to_string())
                .ghost()
                .compact()
                .on_click(cx.listener(move |this, _, _, cx| this.skip(&id, cx))),
        )
        .into_any_element()
    }

    // ---- threads (BIT-T-0454) ---------------------------------------------------------------------

    fn thread_list(&self, bt: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let mut col = v_flex()
            .id("chat-threads")
            .size_full()
            .overflow_y_scroll()
            .gap(m.space[2])
            .p(m.space[4])
            .child(Overline::new(t!("chat.threads").to_string()).count(self.threads.rows.len()));
        if self.threads.loading {
            col = col.child(hint(t!("chat.threads_loading").to_string(), bt));
        }
        if let Some(error) = &self.threads.error {
            col = col.child(banner(error.clone(), bt, true));
        }
        if !self.threads.loading && self.threads.rows.is_empty() && self.threads.error.is_none() {
            col = col.child(hint(t!("chat.threads_empty").to_string(), bt));
        }
        for (ix, row) in self.threads.rows.iter().enumerate() {
            let current = self.thread_id.as_deref() == Some(row.id.as_str());
            let (open_id, delete_id) = (row.id.clone(), row.id.clone());
            let title = row
                .title
                .clone()
                .unwrap_or_else(|| t!("chat.thread_untitled").to_string());
            col = col.child(
                h_flex()
                    .id(("thread", ix))
                    .gap(m.space[3])
                    .items_center()
                    .px(m.space[3])
                    .py(m.space[2])
                    .rounded(m.radius_control)
                    .border_1()
                    .border_color(if current { c.ai_line } else { c.line })
                    .when(current, |d| d.bg(c.ai_bg))
                    .cursor_pointer()
                    .hover(|s| s.bg(c.hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.resume_thread(&open_id, cx)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_color(c.text)
                                    .type_style(&bt.type_scale.ui_small)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_color(c.muted)
                                    .type_style(&bt.type_scale.mono)
                                    .child(row.updated.clone()),
                            ),
                    )
                    .child(
                        IconButton::new(
                            SharedString::from(format!("thread-delete-{ix}")),
                            Glyph::Close,
                        )
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_thread(&delete_id, cx);
                        })),
                    ),
            );
        }
        col.into_any_element()
    }

    // ---- composer (BIT-T-0460) --------------------------------------------------------------------

    fn composer_box(&self, bt: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let m = &bt.metrics;
        let c = &bt.colors;
        let mut chips = h_flex()
            .id("chat-context")
            .flex_wrap()
            .items_center()
            .gap(m.space[2])
            .child(
                div()
                    .text_color(c.muted)
                    .type_style(&bt.type_scale.caption)
                    .child(t!("chat.context").to_string()),
            );
        for (ix, item) in self.contexts.iter().enumerate() {
            let icon = match item.kind {
                ContextKind::Page => Glyph::File,
                ContextKind::Journal => Glyph::Calendar,
                ContextKind::Selection => Glyph::SquareCheck,
            };
            chips = chips.child(
                div()
                    .id(("chat-ctx", ix))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.detach(ix, cx)))
                    .child(Chip::new(item.label()).tone(ChipTone::Accent).icon(icon)),
            );
        }
        chips = chips
            .child(
                div()
                    .id("chat-add-page")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.attach_page(cx)))
                    .child(Chip::new(t!("chat.add_page").to_string()).tone(ChipTone::Dashed)),
            )
            .child(
                div()
                    .id("chat-add-selection")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.attach_selection(cx)))
                    .child(Chip::new(t!("chat.add_selection").to_string()).tone(ChipTone::Dashed)),
            );

        let running = self.model.running;
        let send = div()
            .id("chat-send")
            .flex_none()
            .size(px(SEND_SIDE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(m.radius_control)
            .bg(c.text)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.model.running {
                    this.stop(cx);
                } else {
                    this.submit(window, cx);
                }
            }))
            .child(glyph(
                if running {
                    Glyph::Close
                } else {
                    Glyph::ArrowUp
                },
                m.icon,
                c.bg,
                cx,
            ));
        let mode = Button::new("chat-mode")
            .label(if self.can_edit {
                t!("chat.mode_edit").to_string()
            } else {
                t!("chat.mode_read").to_string()
            })
            .icon(if self.can_edit {
                Glyph::Sparkle
            } else {
                Glyph::File
            })
            .ghost()
            .compact()
            .disabled(running)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_edits(cx)));

        v_flex()
            .id("chat-composer")
            .flex_none()
            .gap(m.space[3])
            .p(m.space[4])
            .border_t_1()
            .border_color(c.line)
            .child(chips)
            .when_some(self.notice.clone(), |d, n| {
                d.child(
                    div()
                        .text_color(c.muted)
                        .type_style(&bt.type_scale.caption)
                        .child(n),
                )
            })
            .when_some(
                match &self.phase {
                    Phase::Unavailable(r) if !self.model.messages.is_empty() => Some(r.clone()),
                    _ => None,
                },
                |d, r| {
                    d.child(banner(
                        t!("chat.unavailable", reason = r).to_string(),
                        bt,
                        true,
                    ))
                },
            )
            .child(
                v_flex()
                    .gap(m.space[2])
                    .p(m.space[3])
                    .rounded(m.radius_panel)
                    .border_1()
                    .border_color(c.line_2)
                    .bg(c.raised)
                    .child(
                        Textarea::new(&self.composer)
                            .appearance(false)
                            .bordered(false),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(mode)
                            .child(send),
                    ),
            )
            .into_any_element()
    }
}

fn op_label(kind: &str) -> String {
    match kind {
        "insert" => t!("chat.op_insert"),
        "update" => t!("chat.op_update"),
        "move" => t!("chat.op_move"),
        "delete" => t!("chat.op_delete"),
        _ => t!("chat.op_property"),
    }
    .to_string()
}

fn denied_text(reason: Option<DenyReason>) -> String {
    match reason {
        None | Some(DenyReason::User) => t!("chat.denied_user"),
        Some(DenyReason::Timeout) => t!("chat.denied_timeout"),
        Some(DenyReason::PanelClosed) => t!("chat.denied_panel"),
        Some(DenyReason::ThreadSwitch) => t!("chat.denied_thread"),
        Some(DenyReason::GraphSwitch) => t!("chat.denied_graph"),
        Some(DenyReason::Quit) => t!("chat.denied_quit"),
        Some(DenyReason::Cancelled) => t!("chat.denied_cancelled"),
        Some(DenyReason::ServerExpired) => t!("chat.denied_expired"),
    }
    .to_string()
}

fn hint(text: String, bt: &BitacoraTheme) -> AnyElement {
    div()
        .text_color(bt.colors.muted)
        .type_style(&bt.type_scale.ui_small)
        .child(text)
        .into_any_element()
}

fn banner(text: String, bt: &BitacoraTheme, warn: bool) -> AnyElement {
    let c = &bt.colors;
    let tint = if warn { c.warn } else { c.muted };
    div()
        .px(bt.metrics.space[3])
        .py(bt.metrics.space[2])
        .rounded(bt.metrics.radius_control)
        .border_1()
        .border_color(tint.opacity(0.4))
        .text_color(tint)
        .type_style(&bt.type_scale.ui_small)
        .child(text)
        .into_any_element()
}

fn code_box(label: String, text: String, bt: &BitacoraTheme) -> AnyElement {
    let m = &bt.metrics;
    let c = &bt.colors;
    v_flex()
        .gap(m.space[1])
        .child(Overline::new(label))
        .child(
            div()
                .p(m.space[3])
                .rounded(m.radius_control)
                .bg(c.bg)
                .border_1()
                .border_color(c.line)
                .text_color(c.text_2)
                .type_style(&bt.type_scale.mono)
                .child(text),
        )
        .into_any_element()
}

fn usage_el(state: &AgentState, bt: &BitacoraTheme) -> Option<AnyElement> {
    let used = state.used_tokens();
    if used == 0 && state.context_window == 0 {
        return None;
    }
    let c = &bt.colors;
    let m = &bt.metrics;
    let approx = if state.estimated { "~" } else { "" };
    let label = if state.context_window > 0 {
        format!(
            "{approx}{} / {}",
            tokens_text(used),
            tokens_text(state.context_window)
        )
    } else {
        format!("{approx}{}", tokens_text(used))
    };
    Some(
        h_flex()
            .id("chat-usage")
            .gap(m.space[2])
            .items_center()
            .when_some(usage_fraction(used, state.context_window), |d, f| {
                d.child(
                    div()
                        .w(dims::PX_40)
                        .h(dims::PX_3)
                        .rounded(m.radius_pill)
                        .bg(c.line)
                        .child(
                            div()
                                .h_full()
                                .w(relative(f))
                                .rounded(m.radius_pill)
                                .bg(c.ai),
                        ),
                )
            })
            .child(
                div()
                    .text_color(c.muted)
                    .type_style(&bt.type_scale.mono)
                    .child(label),
            )
            .into_any_element(),
    )
}

/// The blocks of an answer as elements; links navigate through `nav`.
fn md_el(
    key: usize,
    blocks: &[Rendered],
    bt: &BitacoraTheme,
    theme: &crate::ui::theme::Theme,
    nav: &Nav,
) -> AnyElement {
    let m = &bt.metrics;
    let c = &bt.colors;
    let text = |ix: usize, layout: &TextLayout| {
        text_element_owned(
            ("chat-md", key * 1_000 + ix),
            layout.clone(),
            theme,
            false,
            Some(Rc::clone(nav)),
            None,
        )
    };
    v_flex()
        .gap(m.space[3])
        .children(blocks.iter().enumerate().map(|(ix, block)| {
            match block {
                Rendered::Heading(level, layout) => div()
                    .type_style(match level {
                        1 => &bt.type_scale.h1_block,
                        2 => &bt.type_scale.h2_block,
                        _ => &bt.type_scale.h3_block,
                    })
                    .child(text(ix, layout))
                    .into_any_element(),
                Rendered::Para(layout) => div().child(text(ix, layout)).into_any_element(),
                Rendered::Bullet(depth, layout) => h_flex()
                    .items_start()
                    .gap(m.space[3])
                    .pl(dims::PX_16 * (*depth as f32))
                    .child(div().flex_none().text_color(c.muted).child(BULLET))
                    .child(div().flex_1().min_w_0().child(text(ix, layout)))
                    .into_any_element(),
                Rendered::Quote(layout) => div()
                    .pl(m.space[3])
                    .border_l_2()
                    .border_color(c.line_2)
                    .text_color(c.text_2)
                    .child(text(ix, layout))
                    .into_any_element(),
                Rendered::Code(lang, code) => code_box(lang.clone(), code.clone(), bt),
            }
        }))
        .into_any_element()
}
