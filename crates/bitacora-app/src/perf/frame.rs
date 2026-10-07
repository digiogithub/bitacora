//! Zero-size element placed last in the window: its paint marks the end of the CPU side of a
//! frame, so the benchmark can compute the time between a tick (frame start) and the end of
//! paint without waiting for the display (BIT-T-0337).

use std::sync::Mutex;
use std::time::Instant;

use crate::ui::text_edit::{
    Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Style,
};
use crate::ui::{App, Bounds, Pixels, Window};

static LAST_PAINT_END: Mutex<Option<Instant>> = Mutex::new(None);

/// When the last frame finished painting.
pub fn last_paint_end() -> Option<Instant> {
    LAST_PAINT_END.lock().ok().and_then(|g| *g)
}

/// The marker element.
#[derive(Debug, Default)]
pub struct FrameEnd;

impl IntoElement for FrameEnd {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FrameEnd {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        (window.request_layout(Style::default(), [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
        if let Ok(mut last) = LAST_PAINT_END.lock() {
            *last = Some(Instant::now());
        }
    }
}
