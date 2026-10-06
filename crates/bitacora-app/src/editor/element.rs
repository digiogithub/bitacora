//! `BlockTextElement`: the custom GPUI element that shapes, paints and hit-tests the block in
//! edit mode (soft wrap, selection rectangles, caret) and registers the platform input handler
//! so IME composition works (ADR-002).
//!
//! The structure follows GPUI's `examples/input.rs` (Apache-2.0); the multi-line wrapping,
//! selection geometry and layout hand-off are our own.

use std::rc::Rc;

use super::layout::BlockLayout;
use super::style::{Palette, TextMetrics, source_runs, style_runs};
use super::view::OutlineEditor;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::text_edit::{
    AvailableSpace, CursorStyle, Element, ElementId, ElementInputHandler, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Style, TextAlign, fill, relative,
};
use crate::ui::{
    ActiveTheme as _, AnyElement, App, Bounds, Entity, FluentBuilder as _, InteractiveElement as _,
    ParentElement as _, Pixels, SharedString, Sizable as _, Styled as _, Window, div, h_flex,
    point, px, size, v_flex,
};

/// Element showing the edited block of an [`OutlineEditor`].
#[derive(Debug)]
pub struct BlockTextElement {
    editor: Entity<OutlineEditor>,
}

impl BlockTextElement {
    /// Creates the element for `editor`.
    pub fn new(editor: Entity<OutlineEditor>) -> Self {
        Self { editor }
    }
}

/// Everything prepaint computed for paint.
#[derive(Debug)]
pub struct Prepaint {
    layout: Rc<BlockLayout>,
    selection: Vec<Bounds<Pixels>>,
    caret: Option<Bounds<Pixels>>,
    palette: Palette,
    metrics: TextMetrics,
}

impl IntoElement for BlockTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BlockTextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let metrics = TextMetrics::from_window(window);
        let editor = self.editor.read(cx);
        let text: SharedString = editor.buffer_text().unwrap_or("").to_owned().into();
        let runs = style_runs(
            source_runs(&text, editor.marked_range()),
            &metrics.font,
            &Palette::from_theme(cx),
        );
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _| {
                let width = known.width.or(match available.width {
                    AvailableSpace::Definite(w) => Some(w),
                    _ => None,
                });
                let lines = window
                    .text_system()
                    .shape_text(text.clone(), metrics.font_size, &runs, width, None)
                    .map(|l| l.into_iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                let layout = BlockLayout::new(lines, metrics.line_height);
                size(width.unwrap_or_else(|| px(0.)), layout.height())
            });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let metrics = TextMetrics::from_window(window);
        let palette = Palette::from_theme(cx);
        let editor = self.editor.read(cx);
        let layout = editor.shape_with(
            editor.buffer_text().unwrap_or(""),
            editor.marked_range(),
            bounds.size.width,
            &metrics,
            &palette,
            window,
        );
        let selection_range = editor.selection_range();
        let selection = layout
            .selection_rects(selection_range.clone())
            .into_iter()
            .map(|r| Bounds::new(bounds.origin + r.origin, r.size))
            .collect();
        let caret = selection_range.is_empty().then(|| {
            let p = layout.position_for_index(editor.cursor_offset());
            Bounds::new(bounds.origin + p, size(px(1.5), layout.line_height()))
        });
        Prepaint {
            layout,
            selection,
            caret,
            palette,
            metrics,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle_ref().clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for rect in &prepaint.selection {
            window.paint_quad(fill(*rect, prepaint.palette.selection));
        }
        let line_height = prepaint.layout.line_height();
        let mut row_ix = 0;
        for (line_ix, line) in prepaint.layout.lines().iter().enumerate() {
            let origin = point(bounds.origin.x, bounds.origin.y + line_height * row_ix);
            // Paint failures (missing glyphs) must not take the window down.
            if let Err(err) = line.paint(origin, line_height, TextAlign::Left, None, window, cx) {
                tracing::warn!("painting block text failed: {err}");
            }
            row_ix += prepaint
                .layout
                .rows()
                .iter()
                .filter(|r| r.line == line_ix)
                .count();
        }
        let blink_on = self.editor.read(cx).caret_visible();
        if focus_handle.is_focused(window)
            && blink_on
            && let Some(caret) = prepaint.caret
        {
            window.paint_quad(fill(caret, prepaint.palette.caret));
        }
        let layout = prepaint.layout.clone();
        let metrics = prepaint.metrics.clone();
        self.editor.update(cx, |editor, _| {
            editor.record_paint(layout, bounds, metrics);
        });
    }
}

/// The popup under the caret column: the completion list or the calendar.
#[derive(Debug, Clone)]
pub struct PopupData {
    /// Candidate labels (a window of the list).
    pub labels: Vec<String>,
    /// Highlighted candidate inside `labels`.
    pub selected: usize,
    /// Index of the first label in the whole candidate list.
    pub first: usize,
    /// Caret column inside the text area.
    pub x: Pixels,
    /// The calendar to show instead of the list.
    pub calendar: Option<Entity<crate::ui::calendar::CalendarState>>,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

fn calendar_view(
    state: &Entity<crate::ui::calendar::CalendarState>,
    theme: &crate::ui::theme::Theme,
) -> impl crate::ui::IntoElement {
    use crate::ui::calendar::{Calendar, CalendarItemKind};
    let theme = theme.clone();
    Calendar::new("slash-calendar", state)
        .label(|kind, value| match kind {
            CalendarItemKind::Weekday => WEEKDAYS
                .get(usize::try_from(value).unwrap_or(0) % 7)
                .copied()
                .unwrap_or("")
                .into(),
            CalendarItemKind::MonthToggle => MONTHS
                .get(usize::try_from(value - 1).unwrap_or(0) % 12)
                .copied()
                .unwrap_or("")
                .into(),
            CalendarItemKind::Month => MONTHS
                .get(usize::try_from(value - 1).unwrap_or(0) % 12)
                .map_or("", |m| &m[..3])
                .into(),
            CalendarItemKind::Previous => "\u{2039}".into(),
            CalendarItemKind::Next => "\u{203a}".into(),
            _ => value.to_string().into(),
        })
        .item(move |item, st, _, _| {
            let day = matches!(
                st.kind(),
                CalendarItemKind::Day | CalendarItemKind::Month | CalendarItemKind::Year
            );
            let base = item
                .flex()
                .items_center()
                .justify_center()
                .h(px(26.))
                .rounded(px(4.))
                .text_sm()
                .min_w(px(if day { 30. } else { 26. }));
            let styled = if st.is_active() {
                base.bg(theme.primary).text_color(theme.primary_foreground)
            } else if st.kind() == CalendarItemKind::Weekday || st.is_muted() {
                base.text_color(theme.muted_foreground)
            } else if st.is_today() {
                base.border_1().border_color(theme.primary)
            } else {
                base
            };
            if st.is_disabled() {
                styled.into_any_element()
            } else {
                styled
                    .cursor_pointer()
                    .hover(|d| d.bg(theme.selection))
                    .into_any_element()
            }
        })
}

fn completion_popup(
    editor: &Entity<OutlineEditor>,
    theme: &crate::ui::theme::Theme,
    data: Option<PopupData>,
) -> Option<AnyElement> {
    let PopupData {
        labels: items,
        selected,
        first,
        x,
        calendar,
    } = data?;
    if let Some(state) = calendar {
        let panel = v_flex()
            .absolute()
            .top_full()
            .left(x.max(px(0.)))
            .mt_1()
            .p_2()
            .w(px(240.))
            .rounded(px(6.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(calendar_view(&state, theme));
        return Some(
            crate::ui::deferred(panel)
                .with_priority(1)
                .into_any_element(),
        );
    }
    let mut list = v_flex()
        .absolute()
        .top_full()
        .left(x.max(px(0.)))
        .mt_1()
        .w(px(360.))
        .max_h(px(240.))
        .overflow_hidden()
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .shadow_md()
        .py_1();
    for (ix, label) in items.into_iter().enumerate() {
        let ed = editor.clone();
        let absolute = first + ix;
        list = list.child(
            div()
                .id(("completion", ix))
                .px_2()
                .py(px(2.))
                .text_sm()
                .truncate()
                .cursor_pointer()
                .when(ix == selected, |d| d.bg(theme.selection))
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    ed.update(cx, |this, cx| this.accept_completion(absolute, window, cx));
                }),
        );
    }
    // Painted after the rows below it, so the popup floats over them instead of pushing them.
    Some(
        crate::ui::deferred(list)
            .with_priority(1)
            .into_any_element(),
    )
}

/// The edit-mode content of a row: the text element with mouse handling for the caret.
pub fn edit_content(
    editor: Entity<OutlineEditor>,
    theme: &crate::ui::theme::Theme,
    popup: Option<PopupData>,
) -> AnyElement {
    let popup = completion_popup(&editor, theme, popup);
    let down = editor.clone();
    let moved = editor.clone();
    let up = editor.clone();
    v_flex()
        .w_full()
        .relative()
        .child(
            div()
                .w_full()
                .cursor(CursorStyle::IBeam)
                .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
                    down.update(cx, |this, cx| {
                        this.edit_mouse_down(
                            e.position,
                            e.click_count,
                            e.modifiers.shift,
                            window,
                            cx,
                        );
                    });
                    cx.stop_propagation();
                })
                .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
                    moved.update(cx, |this, cx| this.edit_mouse_move(e.position, cx));
                })
                .on_mouse_up(MouseButton::Left, move |_: &MouseUpEvent, _, cx| {
                    up.update(cx, |this, _| this.edit_mouse_up());
                })
                .child(BlockTextElement::new(editor)),
        )
        .children(popup)
        .into_any_element()
}

/// The "this block changed on disk" choice shown above the editor.
pub fn conflict_bar(editor: Entity<OutlineEditor>) -> AnyElement {
    let keep = editor.clone();
    let take = editor;
    h_flex()
        .gap_2()
        .items_center()
        .px_2()
        .py_1()
        .text_xs()
        .child("This block changed on disk while you were editing it.")
        .child(
            Button::new("keep-mine")
                .small()
                .label("Keep mine")
                .on_click(move |_, window, cx| {
                    keep.update(cx, |this, cx| this.resolve_conflict(true, window, cx));
                }),
        )
        .child(
            Button::new("take-disk")
                .small()
                .ghost()
                .label("Take disk")
                .on_click(move |_, window, cx| {
                    take.update(cx, |this, cx| this.resolve_conflict(false, window, cx));
                }),
        )
        .into_any_element()
}

/// Wraps `content` (the page list) with the outline key context, focus tracking and the
/// editor actions.
pub fn wrap(content: AnyElement, editor: &Entity<OutlineEditor>, cx: &App) -> AnyElement {
    let ed = editor.read(cx);
    let _ = cx.theme();
    let up = editor.clone();
    let up_out = editor.clone();
    let dropped = editor.clone();
    let scrolled = editor.clone();
    let ended = editor.clone();
    let container = div()
        .size_full()
        .key_context(ed.key_context_name())
        .track_focus(ed.focus_handle_ref())
        .on_drop(
            move |paths: &crate::ui::text_edit::ExternalPaths, window, cx| {
                dropped.update(cx, |this, cx| {
                    this.drop_files_on_page(paths.paths(), window, cx);
                });
            },
        )
        .on_drag_move::<super::dnd::BlockDrag>(move |e, _, cx| {
            // Near the top or bottom edge of the page area the page scrolls (BIT-US-0106).
            scrolled.update(cx, |this, cx| {
                this.autoscroll(e.event.position, e.bounds, cx)
            });
        })
        .on_drop::<super::dnd::BlockDrag>(move |_, _, cx| {
            ended.update(cx, |this, cx| this.drag_cancel(cx));
        })
        .on_mouse_up(crate::ui::text_edit::MouseButton::Left, move |_, _, cx| {
            up.update(cx, |this, _| this.drag_end());
        })
        .on_mouse_up_out(crate::ui::text_edit::MouseButton::Left, move |_, _, cx| {
            up_out.update(cx, |this, _| this.drag_end());
        })
        .child(content);
    super::view::attach(container, editor).into_any_element()
}
