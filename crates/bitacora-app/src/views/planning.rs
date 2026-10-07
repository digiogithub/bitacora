//! The `SCHEDULED` / `DEADLINE` date chip and its date picker (BIT-US-0167).
//!
//! The chip is shared by the page outline, the Tasks view and the references. It owns no state:
//! the host view keeps which chip is open and which month it shows, hands that to the chip as
//! [`PlanningActions`] and applies the picked day through the core command queue (one undoable
//! transaction that rewrites only the planning line, see
//! `bitacora_markdown::edit::state::move_planning_date`).

use std::rc::Rc;

use bitacora_core::date::Date;
use rust_i18n::t;

use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    Anchor, AnyElement, App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, anchored,
    deferred, div, h_flex,
};
use crate::views::calendar::{CalendarHandlers, CalendarState, Month, render_calendar};
use crate::views::dims;
use crate::views::kit::{Button, Glyph, PopoverShell, glyph};

/// A planning keyword as written in the file.
pub const SCHEDULED: &str = "SCHEDULED:";
/// A planning keyword as written in the file.
pub const DEADLINE: &str = "DEADLINE:";

/// Maps the keyword shown on a chip (`SCHEDULED`) to the one in the file (`SCHEDULED:`);
/// `None` for `CLOSED` and unknown words (not editable).
#[must_use]
pub fn file_keyword(shown: &str) -> Option<&'static str> {
    match shown.trim_end_matches(':') {
        "SCHEDULED" => Some(SCHEDULED),
        "DEADLINE" => Some(DEADLINE),
        _ => None,
    }
}

/// `(year, month, day)` of a chip's timestamp text (`<2025-11-20 Thu .+1d>`).
#[must_use]
pub fn chip_date(text: &str) -> Option<(i32, u32, u32)> {
    let inner = text.trim().trim_start_matches(['<', '[']);
    let date = inner.get(..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
}

/// Splits a `yyyyMMdd` key.
#[must_use]
pub fn split_key(key: u32) -> (u32, u32, u32) {
    (key / 10_000, key / 100 % 100, key % 100)
}

/// The picker that is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPicker {
    /// Keyword of the chip it belongs to.
    pub keyword: &'static str,
    /// Displayed month.
    pub month: Month,
}

/// Callback with a keyword.
pub type KeywordHook = Rc<dyn Fn(&'static str, &mut Window, &mut App)>;
/// Callback with a keyword and a `yyyyMMdd` day.
pub type PickHook = Rc<dyn Fn(&'static str, u32, &mut Window, &mut App)>;

/// What a chip can do, built by the host view for one block.
#[derive(Clone)]
pub struct PlanningActions {
    /// The picker that is open for this block, if any.
    pub open: Option<OpenPicker>,
    /// Today, for the calendar's highlight.
    pub today: Option<Date>,
    /// The chip was clicked: open its picker.
    pub on_open: KeywordHook,
    /// A month arrow: `-1` / `+1`.
    pub on_shift: crate::views::calendar::ShiftCallback,
    /// A day was picked.
    pub on_pick: PickHook,
    /// "Remove date".
    pub on_clear: KeywordHook,
    /// Escape or a click outside.
    pub on_close: crate::views::block_view::Action,
}

impl std::fmt::Debug for PlanningActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanningActions")
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

/// One chip: `SCHEDULED <2025-11-20 Thu>` with a calendar icon. Clickable (and anchoring the
/// picker) when `actions` is given and the keyword is editable.
#[derive(IntoElement, Debug)]
pub struct PlanningChipView {
    id: SharedString,
    keyword: &'static str,
    shown: String,
    text: String,
    actions: Option<PlanningActions>,
    label: Option<String>,
}

impl PlanningChipView {
    /// `shown` is the chip's keyword (`SCHEDULED`), `text` its timestamp.
    pub fn new(
        id: impl Into<SharedString>,
        shown: &str,
        text: &str,
        actions: Option<PlanningActions>,
    ) -> Self {
        let keyword = file_keyword(shown);
        Self {
            id: id.into(),
            keyword: keyword.unwrap_or(""),
            shown: shown.to_owned(),
            text: text.to_owned(),
            actions: actions.filter(|_| keyword.is_some()),
            label: None,
        }
    }
}

impl PlanningChipView {
    /// Replaces the `KEYWORD <timestamp>` text of the chip (the Tasks view words it).
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for PlanningChipView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let bt = cx.bitacora().clone();
        let c = &bt.colors;
        let selector = format!("planning-chip-{}", self.id);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| format!("{} {}", self.shown, self.text));
        let mut chip = h_flex()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .relative()
            .gap_1()
            .px(dims::PX_6)
            .rounded(dims::PX_4)
            .bg(c.hover)
            .text_color(c.text_2)
            .type_style(&bt.type_scale.caption)
            .child(glyph(Glyph::Calendar, bt.metrics.icon_sm, c.muted, cx))
            .child(label);
        let Some(actions) = self.actions else {
            return chip.into_any_element();
        };
        let keyword = self.keyword;
        let on_open = actions.on_open.clone();
        chip = chip
            .cursor_pointer()
            .hover(|s| s.bg(c.line))
            .on_mouse_down(crate::ui::text_edit::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                on_open(keyword, window, cx);
            });
        if let Some(open) = actions.open.filter(|o| o.keyword == keyword) {
            chip = chip.child(picker_popover(
                &self.id, keyword, &self.text, open, &actions, cx,
            ));
        }
        chip.into_any_element()
    }
}

fn picker_popover(
    id: &str,
    keyword: &'static str,
    text: &str,
    open: OpenPicker,
    actions: &PlanningActions,
    cx: &mut App,
) -> AnyElement {
    let bt = cx.bitacora().clone();
    let current = chip_date(text)
        .and_then(|(y, m, d)| u32::try_from(y).ok().map(|y| y * 10_000 + m * 100 + d));
    let state = CalendarState {
        month: open.month,
        today: actions.today,
        selected: current,
        with_notes: Default::default(),
    };
    let on_pick = actions.on_pick.clone();
    let on_shift = actions.on_shift.clone();
    let handlers = CalendarHandlers {
        on_day: Rc::new(move |key, window, cx| on_pick(keyword, key, window, cx)),
        on_shift: Rc::new(move |delta, window, cx| on_shift(delta, window, cx)),
    };
    let on_clear = actions.on_clear.clone();
    let on_close = actions.on_close.clone();
    let body = crate::ui::v_flex()
        .gap(bt.metrics.space[2])
        .p(bt.metrics.space[2])
        .child(render_calendar(&state, &handlers, cx))
        .child(
            h_flex().justify_end().child(
                Button::new(SharedString::from(format!("planning-remove-{id}")))
                    .ghost()
                    .label(t!("planning.remove").to_string())
                    .on_click(move |_, window, cx| on_clear(keyword, window, cx)),
            ),
        );
    deferred(
        anchored().anchor(Anchor::TopLeft).snap_to_window().child(
            div().mt(bt.metrics.space[1]).child(
                PopoverShell::new(SharedString::from(format!("planning-picker-{id}")))
                    .width(dims::PX_280)
                    .on_dismiss(move |window, cx| on_close(window, cx))
                    .child(body),
            ),
        ),
    )
    .priority(10)
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_date_of_a_chip() {
        assert_eq!(chip_date("<2025-11-20 Thu .+1d>"), Some((2025, 11, 20)));
        assert_eq!(chip_date("[2025-01-02]"), Some((2025, 1, 2)));
        assert_eq!(chip_date("<soon>"), None);
    }

    #[test]
    fn only_scheduled_and_deadline_are_editable() {
        assert_eq!(file_keyword("SCHEDULED"), Some(SCHEDULED));
        assert_eq!(file_keyword("DEADLINE:"), Some(DEADLINE));
        assert_eq!(file_keyword("CLOSED"), None);
    }

    #[test]
    fn splits_a_day_key() {
        assert_eq!(split_key(20_251_121), (2025, 11, 21));
    }
}
