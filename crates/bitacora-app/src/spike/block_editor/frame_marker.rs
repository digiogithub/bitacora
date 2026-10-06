//! Zero-size element placed after the list: its paint marks the end of the CPU side of
//! a frame so the benchmark can compute render-to-paint time (BIT-T-0090).

use super::editor::SpikeEditor;
use crate::ui::text_edit::{
    Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Style,
};
use crate::ui::{App, Bounds, Entity, Pixels, Window};

/// Marks the end of a frame on `editor`.
#[derive(Debug)]
pub struct FrameEndMarker {
    editor: Entity<SpikeEditor>,
}

impl FrameEndMarker {
    /// Creates the marker for `editor`.
    pub fn new(editor: Entity<SpikeEditor>) -> Self {
        Self { editor }
    }
}

impl IntoElement for FrameEndMarker {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FrameEndMarker {
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
        cx: &mut App,
    ) {
        self.editor.update(cx, |editor, _| editor.mark_frame_end());
    }
}
