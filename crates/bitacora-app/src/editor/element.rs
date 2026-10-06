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

/// The completion popup under the caret column, when one is open.
#[derive(Debug, Clone)]
pub struct PopupData {
    /// Candidate labels.
    pub labels: Vec<String>,
    /// Highlighted candidate.
    pub selected: usize,
    /// Caret column inside the text area.
    pub x: Pixels,
}

fn completion_popup(
    editor: &Entity<OutlineEditor>,
    theme: &crate::ui::theme::Theme,
    data: Option<PopupData>,
) -> Option<AnyElement> {
    let PopupData {
        labels: items,
        selected,
        x,
    } = data?;
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
                    ed.update(cx, |this, cx| this.accept_completion(ix, window, cx));
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
    let container = div()
        .size_full()
        .key_context(ed.key_context_name())
        .track_focus(ed.focus_handle_ref())
        .on_mouse_up(crate::ui::text_edit::MouseButton::Left, move |_, _, cx| {
            up.update(cx, |this, _| this.drag_end());
        })
        .on_mouse_up_out(crate::ui::text_edit::MouseButton::Left, move |_, _, cx| {
            up_out.update(cx, |this, _| this.drag_end());
        })
        .child(content);
    super::view::attach(container, editor).into_any_element()
}
