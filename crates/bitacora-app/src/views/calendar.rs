//! Month calendar of the left sidebar (design `componentes.md` "Calendario", `layout.md`).
//!
//! The grid maths are pure functions ([`month_grid`], [`shift_month`], [`month_bounds`],
//! [`cell_look`]) so tests need no window. [`render_calendar`] paints a [`CalendarState`] and
//! reports clicks through callbacks; it owns no state.

use crate::views::dims;
use std::collections::HashSet;
use std::rc::Rc;

use bitacora_core::date::Date;
use rust_i18n::t;

use crate::ui::text_edit::FontWeight;
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, TypeStyleExt as _};
use crate::ui::{
    AnyElement, App, ClickEvent, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div, h_flex, px,
    v_flex,
};
use crate::views::kit::{Glyph, glyph};

/// Diameter of the "has notes" dot (`layout.md`: 4 px).
const DOT: f32 = 4.0;

/// A displayed month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Month {
    /// Full year.
    pub year: i32,
    /// 1-12.
    pub month: u8,
}

impl Month {
    /// The month containing `date`.
    pub fn of(date: Date) -> Self {
        Self {
            year: date.year(),
            month: date.month(),
        }
    }
}

/// Moves `month` by `delta` months (negative = earlier), staying inside years 0..=9999.
#[must_use]
pub fn shift_month(month: Month, delta: i32) -> Month {
    let index = month.year * 12 + i32::from(month.month) - 1 + delta;
    let index = index.clamp(0, 9999 * 12 + 11);
    Month {
        year: index.div_euclid(12),
        month: (index.rem_euclid(12) + 1) as u8,
    }
}

/// First and last `yyyyMMdd` of the month.
#[must_use]
pub fn month_bounds(month: Month) -> (u32, u32) {
    let first = month.year as u32 * 10_000 + u32::from(month.month) * 100 + 1;
    let last = Date::new(month.year, month.month, 1)
        .and_then(|d| {
            // Last day = day before the first of the next month.
            let next = shift_month(month, 1);
            let next_first = Date::new(next.year, next.month, 1)?;
            if next == month {
                return Some(d);
            }
            next_first.add_days(-1)
        })
        .map(Date::journal_day)
        .unwrap_or(first);
    (first, last)
}

/// The month as a grid of weeks, Monday first. `None` cells pad the first and last week.
#[must_use]
pub fn month_grid(month: Month) -> Vec<[Option<u8>; 7]> {
    let Some(first) = Date::new(month.year, month.month, 1) else {
        return Vec::new();
    };
    let (_, last) = month_bounds(month);
    let days = (last % 100) as u8;
    let lead = first.weekday();
    let mut rows = Vec::new();
    let mut row = [None; 7];
    let mut col = lead;
    for day in 1..=days {
        row[col] = Some(day);
        col += 1;
        if col == 7 {
            rows.push(row);
            row = [None; 7];
            col = 0;
        }
    }
    if col != 0 {
        rows.push(row);
    }
    rows
}

/// What a calendar needs to paint.
#[derive(Debug, Clone)]
pub struct CalendarState {
    /// Displayed month.
    pub month: Month,
    /// Today (local), when known.
    pub today: Option<Date>,
    /// The journal day on screen (`yyyyMMdd`).
    pub selected: Option<u32>,
    /// Days of the displayed month with journal content (`yyyyMMdd`).
    pub with_notes: HashSet<u32>,
}

/// Colours of one day cell: `(fill, text, weight, dot)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellLook {
    /// Background, none for a plain day.
    pub fill: Option<Hsla>,
    /// Number colour.
    pub text: Hsla,
    /// Number weight.
    pub weight: FontWeight,
    /// Dot colour; `None` reserves the room without painting.
    pub dot: Option<Hsla>,
}

/// Resolves the look of day `key` (`yyyyMMdd`). The selected style wins over today's; future
/// days are `muted`.
#[must_use]
pub fn cell_look(key: u32, state: &CalendarState, theme: &BitacoraTheme) -> CellLook {
    let c = &theme.colors;
    let is_today = state.today.map(Date::journal_day) == Some(key);
    let is_future = state.today.is_some_and(|t| key > t.journal_day());
    let has_notes = state.with_notes.contains(&key);
    if state.selected == Some(key) {
        CellLook {
            fill: Some(c.accent),
            text: c.on_accent,
            weight: FontWeight::SEMIBOLD,
            dot: has_notes.then_some(c.on_accent),
        }
    } else if is_today {
        CellLook {
            fill: Some(c.accent_bg),
            text: c.accent,
            weight: FontWeight::BOLD,
            dot: has_notes.then_some(c.accent),
        }
    } else {
        CellLook {
            fill: None,
            text: if is_future { c.muted } else { c.text_2 },
            weight: FontWeight::NORMAL,
            dot: has_notes.then_some(c.accent),
        }
    }
}

/// Callbacks of [`render_calendar`].
#[derive(Clone)]
pub struct CalendarHandlers {
    /// A day (`yyyyMMdd`) was clicked.
    pub on_day: CalendarCallback,
    /// The month arrows: `-1` previous, `+1` next.
    pub on_shift: ShiftCallback,
}

/// Callback with a `yyyyMMdd` day.
pub type CalendarCallback = Rc<dyn Fn(u32, &mut Window, &mut App)>;
/// Callback with a month delta.
pub type ShiftCallback = Rc<dyn Fn(i32, &mut Window, &mut App)>;

impl std::fmt::Debug for CalendarHandlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalendarHandlers").finish_non_exhaustive()
    }
}

/// Localised name of `month` (1-12), from `calendar.months`.
#[must_use]
pub fn month_name(month: u8) -> String {
    let names = t!("calendar.months").to_string();
    names
        .split(',')
        .nth(usize::from(month.saturating_sub(1)))
        .map_or_else(|| month.to_string(), |s| capitalize(s.trim()))
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Paints the calendar box: month header with arrows, weekday initials and the day grid.
pub fn render_calendar(
    state: &CalendarState,
    handlers: &CalendarHandlers,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.bitacora().clone();
    let m = &theme.metrics;
    let c = &theme.colors;
    let arrow = |id: &'static str, glyph_kind: Glyph, delta: i32, cx: &mut App| {
        let on_shift = handlers.on_shift.clone();
        let colour = c.muted;
        div()
            .id(ElementId::from(id))
            .debug_selector(|| id.to_string())
            .flex()
            .items_center()
            .justify_center()
            .size(m.calendar_cell)
            .rounded(m.radius_control)
            .cursor_pointer()
            .hover(|s| s.bg(c.hover))
            .on_click(move |_: &ClickEvent, window, cx| on_shift(delta, window, cx))
            .child(glyph(glyph_kind, m.icon_sm, colour, cx))
    };
    let header = h_flex()
        .items_center()
        .justify_between()
        .child(arrow("calendar-prev", Glyph::ChevronLeft, -1, cx))
        .child(
            div()
                .text_color(c.text)
                .type_style(&theme.type_scale.ui)
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!(
                    "{} {}",
                    month_name(state.month.month),
                    state.month.year
                )),
        )
        .child(arrow("calendar-next", Glyph::ChevronRight, 1, cx));
    let weekdays = t!("calendar.weekdays").to_string();
    let heads = h_flex().children(weekdays.split_whitespace().map(|w| {
        div()
            .flex()
            .items_center()
            .justify_center()
            .flex_1()
            .h(m.calendar_cell)
            .text_color(c.muted)
            .type_style(&theme.type_scale.caption)
            .child(w.to_string())
    }));
    let mut grid = v_flex();
    for row in month_grid(state.month) {
        let mut line = h_flex();
        for cell in row {
            let Some(day) = cell else {
                line = line.child(div().flex_1().h(m.calendar_cell));
                continue;
            };
            let key = state.month.year as u32 * 10_000
                + u32::from(state.month.month) * 100
                + u32::from(day);
            let look = cell_look(key, state, &theme);
            let on_day = handlers.on_day.clone();
            line = line.child(
                div()
                    .id(ElementId::from(("calendar-day", key as usize)))
                    .debug_selector(|| format!("calendar-day-{key}"))
                    .flex_1()
                    .h(m.calendar_cell)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(dims::PX_1)
                    .rounded(m.radius_control)
                    .cursor_pointer()
                    .when_some(look.fill, |d, fill| d.bg(fill))
                    .when(look.fill.is_none(), |d| d.hover(|s| s.bg(c.hover)))
                    .text_color(look.text)
                    .type_style(&theme.type_scale.ui_small)
                    .font_weight(look.weight)
                    .on_click(move |_: &ClickEvent, window, cx| on_day(key, window, cx))
                    .child(day.to_string())
                    // The dot's room is always reserved so numbers do not jump.
                    .child(
                        div()
                            .size(px(DOT))
                            .rounded_full()
                            .when_some(look.dot, |d, colour| d.bg(colour)),
                    ),
            );
        }
        grid = grid.child(line);
    }
    v_flex()
        .p(m.space[4])
        .rounded(m.radius_panel)
        .border_1()
        .border_color(c.line)
        .bg(c.panel)
        .child(header)
        .child(heads)
        .child(grid)
        .into_any_element()
}

use crate::ui::FluentBuilder as _;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Mode;

    fn month(year: i32, month: u8) -> Month {
        Month { year, month }
    }

    #[test]
    fn october_2026_starts_on_thursday_monday_first() {
        let grid = month_grid(month(2026, 10));
        assert_eq!(grid.len(), 5);
        // 1 Oct 2026 is a Thursday: columns Mon..Wed are padding.
        assert_eq!(
            grid[0],
            [None, None, None, Some(1), Some(2), Some(3), Some(4)]
        );
        assert_eq!(grid[4][5], Some(31));
    }

    #[test]
    fn grid_covers_every_day_exactly_once() {
        for (y, m, days) in [(2026, 2, 28), (2024, 2, 29), (2026, 12, 31), (2026, 4, 30)] {
            let cells: Vec<u8> = month_grid(month(y, m))
                .iter()
                .flatten()
                .flatten()
                .copied()
                .collect();
            assert_eq!(cells, (1..=days).collect::<Vec<u8>>(), "{y}-{m}");
        }
        // February 2027 starts on Monday and fills exactly four rows.
        assert_eq!(month_grid(month(2027, 2)).len(), 4);
    }

    #[test]
    fn shift_wraps_years_and_bounds() {
        assert_eq!(shift_month(month(2026, 1), -1), month(2025, 12));
        assert_eq!(shift_month(month(2026, 12), 1), month(2027, 1));
        assert_eq!(shift_month(month(2026, 10), -22), month(2024, 12));
        assert_eq!(shift_month(month(0, 1), -1), month(0, 1));
    }

    #[test]
    fn bounds_cover_the_month() {
        assert_eq!(month_bounds(month(2026, 10)), (20_261_001, 20_261_031));
        assert_eq!(month_bounds(month(2024, 2)), (20_240_201, 20_240_229));
        assert_eq!(month_bounds(month(2026, 12)), (20_261_201, 20_261_231));
    }

    fn theme(dark: bool) -> BitacoraTheme {
        BitacoraTheme::new(if dark { Mode::Dark } else { Mode::Light })
    }

    #[test]
    fn cell_looks_follow_the_design() {
        for dark in [false, true] {
            let theme = theme(dark);
            let c = theme.colors;
            let state = CalendarState {
                month: month(2026, 10),
                today: Date::new(2026, 10, 7),
                selected: Some(20_261_003),
                with_notes: [20_261_003, 20_261_007, 20_261_010, 20_261_001].into(),
            };
            // Selected: accent fill, on_accent text.
            let sel = cell_look(20_261_003, &state, &theme);
            assert_eq!(
                (sel.fill, sel.text, sel.dot),
                (Some(c.accent), c.on_accent, Some(c.on_accent))
            );
            // Today: accent_bg fill, accent bold text.
            let today = cell_look(20_261_007, &state, &theme);
            assert_eq!((today.fill, today.text), (Some(c.accent_bg), c.accent));
            assert_eq!(today.weight, FontWeight::BOLD);
            // Future day with notes: muted, dot kept.
            let future = cell_look(20_261_010, &state, &theme);
            assert_eq!(
                (future.fill, future.text, future.dot),
                (None, c.muted, Some(c.accent))
            );
            // Past day without notes: no dot.
            let past = cell_look(20_261_002, &state, &theme);
            assert_eq!((past.fill, past.text, past.dot), (None, c.text_2, None));
        }
    }

    #[test]
    fn month_names_localise() {
        assert_eq!(month_name(10), "October");
    }
}
