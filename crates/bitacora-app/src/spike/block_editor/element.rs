//! `BlockTextElement`: the custom GPUI element that shapes, paints and hit-tests the
//! focused block (soft wrap, selection rectangles, caret) and registers the platform
//! input handler so IME composition works (ADR-002, option C).
//!
//! The overall structure follows GPUI's `examples/input.rs` (Apache-2.0); the
//! multi-line wrapping, selection geometry and layout cache handoff are our own.

use std::rc::Rc;

use super::editor::{Palette, SpikeEditor, TextMetrics, style_runs};
use super::inline::source_runs;
use super::layout::BlockLayout;
use crate::ui::text_edit::{
    AvailableSpace, Element, ElementId, ElementInputHandler, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Style, TextAlign, fill, relative,
};
use crate::ui::{App, Bounds, Entity, Pixels, SharedString, Window, point, px, size};

/// Element showing the focused block of a [`SpikeEditor`].
#[derive(Debug)]
pub struct BlockTextElement {
    editor: Entity<SpikeEditor>,
}

impl BlockTextElement {
    /// Creates the element for `editor`.
    pub fn new(editor: Entity<SpikeEditor>) -> Self {
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
        let text: SharedString = editor.focused_text().to_owned().into();
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
        let layout = {
            let editor = self.editor.read(cx);
            editor.shape_with(
                editor.focused_text(),
                editor.marked_range(),
                bounds.size.width,
                &metrics,
                &palette,
                window,
            )
        };
        let editor = self.editor.read(cx);
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
        let focus_handle = self.editor.read(cx).focus_handle().clone();
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
