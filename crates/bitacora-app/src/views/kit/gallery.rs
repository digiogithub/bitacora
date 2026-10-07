//! The component gallery: every kit component in every variant on one scrolling page.
//!
//! It is the story used to check the kit by eye (open it in a window with
//! `Gallery::new`) and the fixture of the view tests below. It is not wired into the
//! application chrome.

use crate::views::dims;
use bitacora_markdown::tasks::head::Marker;

use super::{
    Button, ButtonSize, Card, Chip, ChipTone, Glyph, IconButton, Kbd, Overline, Pill, PopoverShell,
    Segmented, Surface, Tab, TaskMarker,
};
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    Context, Div, FluentBuilder as _, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, h_flex,
    v_flex,
};

/// State of the gallery: what the interactive samples last did.
pub struct Gallery {
    /// Whether the popover sample is open.
    pub popover_open: bool,
    /// Key of the selected segment.
    pub segment: SharedString,
    /// Whether the "all" filter pill is the active one.
    pub pill_all: bool,
    /// Active tab index.
    pub tab: usize,
    /// Presses of the primary button.
    pub primary_clicks: usize,
    /// Presses of the disabled button (must stay 0).
    pub disabled_clicks: usize,
}

impl Gallery {
    pub fn new() -> Self {
        Self {
            popover_open: false,
            segment: "list".into(),
            pill_all: true,
            tab: 0,
            primary_clicks: 0,
            disabled_clicks: 0,
        }
    }
}

impl Default for Gallery {
    fn default() -> Self {
        Self::new()
    }
}

/// Sample copy of the story. The gallery is a developer page, not a shipped screen, so its text
/// is not localised.
fn sample(text: &'static str) -> SharedString {
    text.into()
}

fn section(title: &'static str, theme_gap: crate::ui::Pixels) -> Div {
    v_flex().gap(theme_gap).child(Overline::new(title))
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let gap = theme.metrics.space[7];
        let row_gap = theme.metrics.space[5];
        let view = cx.entity().downgrade();
        let seg_view = view.clone();

        let buttons = section("Buttons", row_gap)
            .child(
                h_flex()
                    .gap(row_gap)
                    .child(
                        Button::new("gallery-primary")
                            .label(sample("Primary"))
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.primary_clicks += 1;
                                cx.notify();
                            })),
                    )
                    .child(Button::new("gallery-secondary").label(sample("Secondary")))
                    .child(Button::new("gallery-ghost").label(sample("Ghost")).ghost())
                    .child(
                        Button::new("gallery-ai")
                            .icon(Glyph::Sparkle)
                            .label(sample("Insert below"))
                            .ai()
                            .size(ButtonSize::Compact),
                    )
                    .child(
                        Button::new("gallery-disabled")
                            .label(sample("Disabled"))
                            .primary()
                            .disabled(true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.disabled_clicks += 1;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap(row_gap)
                    .child(IconButton::new("gallery-icon", Glyph::Search))
                    .child(IconButton::new("gallery-icon-sm", Glyph::PanelRight).small())
                    .child(IconButton::new("gallery-icon-on", Glyph::PanelLeft).active(true))
                    .child(IconButton::new("gallery-icon-off", Glyph::Printer).disabled(true)),
            )
            .child(h_flex().gap(row_gap).children(Glyph::ALL.map(|g| {
                IconButton::new(SharedString::from(format!("gallery-glyph-{g:?}")), g).small()
            })));

        let inline = section("Inline", row_gap)
            .child(
                h_flex()
                    .gap(row_gap)
                    .items_center()
                    .child(Kbd::new("Tab"))
                    .child(Kbd::new("⌥]"))
                    .child(Chip::new("#project").tone(ChipTone::Accent))
                    .child(Chip::new("Journal 6 Oct").tone(ChipTone::Neutral))
                    .child(Chip::new("claude").tone(ChipTone::Ai).icon(Glyph::Sparkle))
                    .child(Chip::new("Done").tone(ChipTone::Outline))
                    .child(Chip::new("+ context").tone(ChipTone::Dashed)),
            )
            .child(
                h_flex()
                    .gap(row_gap)
                    .items_center()
                    .children(Marker::ALL.map(TaskMarker::new)),
            )
            .child(
                h_flex()
                    .gap(row_gap)
                    .child(
                        Pill::new("gallery-pill-all", "All")
                            .count(12)
                            .active(self.pill_all)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pill_all = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Pill::new("gallery-pill-open", "Open")
                            .count(5)
                            .active(!self.pill_all)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pill_all = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap(gap)
                    .child(Overline::new("Overdue").count(2).warn(true))
                    .child(Overline::new("This week").count(7)),
            );

        let containers = section("Containers", row_gap)
            .child(
                h_flex()
                    .items_end()
                    .border_b_1()
                    .border_color(theme.colors.line)
                    .children((0..3).map(|ix| {
                        Tab::new(
                            SharedString::from(format!("gallery-tab-{ix}")),
                            format!("Page {ix}"),
                        )
                        .active(self.tab == ix)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tab = ix;
                            cx.notify();
                        }))
                        .on_close(|_, _, _| {})
                    })),
            )
            .child(
                Segmented::new("gallery-segmented")
                    .option("list", "List")
                    .option("grid", "Grid")
                    .selected(self.segment.clone())
                    .on_change(move |key, _, cx| {
                        let key = key.clone();
                        seg_view
                            .update(cx, |this, cx| {
                                this.segment = key;
                                cx.notify();
                            })
                            .ok();
                    }),
            )
            .child(
                h_flex()
                    .gap(row_gap)
                    .child(
                        Card::new()
                            .child(
                                div()
                                    .type_style(&theme.type_scale.caption)
                                    .child(sample("Raised")),
                            )
                            .child(
                                div()
                                    .type_style(&theme.type_scale.panel_body)
                                    .child(sample("A task card")),
                            ),
                    )
                    .child(
                        Card::new().surface(Surface::Panel).child(
                            div()
                                .type_style(&theme.type_scale.panel_body)
                                .child(sample("A task row")),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .gap(row_gap)
                    .child(
                        Button::new("gallery-popover-toggle")
                            .label(sample("Popover"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.popover_open = !this.popover_open;
                                cx.notify();
                            })),
                    )
                    .when(self.popover_open, |d| {
                        d.child(
                            PopoverShell::new("gallery-popover")
                                .width(dims::PX_240)
                                .on_dismiss(move |_, cx| {
                                    view.update(cx, |this, cx| {
                                        this.popover_open = false;
                                        cx.notify();
                                    })
                                    .ok();
                                })
                                .child(
                                    div()
                                        .type_style(&theme.type_scale.ui)
                                        .child(sample("Popover content")),
                                ),
                        )
                    }),
            );

        v_flex()
            .id("kit-gallery")
            .size_full()
            .overflow_y_scroll()
            .gap(gap)
            .p(theme.metrics.space[9])
            .bg(theme.colors.bg)
            .text_color(theme.colors.text)
            .type_style(&theme.type_scale.ui)
            .child(buttons)
            .child(inline)
            .child(containers)
    }
}

opaque_debug!(Gallery);
