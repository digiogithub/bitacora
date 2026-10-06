//! Drag and drop of blocks (BIT-US-0106): the drag payload, the drop zones of a row and the
//! drag preview. The editor state machine is in `view/dnd.rs`; the rows draw the indicator and
//! forward the events (`views::block_view`).

use bitacora_core::editor::{BlockId, Target};

use crate::ui::{
    ActiveTheme as _, Bounds, Context, IntoElement, ParentElement as _, Pixels, Point, Render,
    SharedString, Styled as _, Window, div, px,
};

/// Distance from the viewport edge where auto-scroll starts.
pub const EDGE: f32 = 56.;
/// Pixels scrolled per tick at the very edge.
pub const MAX_STEP: f32 = 22.;

/// The auto-scroll step for a pointer at height `y` in `viewport`: negative near the top edge,
/// positive near the bottom one, growing as the pointer gets closer to it, zero elsewhere.
#[must_use]
pub fn scroll_step(y: Pixels, viewport: Bounds<Pixels>) -> f32 {
    let top = viewport.origin.y.as_f32();
    let bottom = top + viewport.size.height.as_f32();
    let y = y.as_f32();
    if y < top + EDGE {
        -((top + EDGE - y) / EDGE).clamp(0., 1.) * MAX_STEP
    } else if y > bottom - EDGE {
        ((y - (bottom - EDGE)) / EDGE).clamp(0., 1.) * MAX_STEP
    } else {
        0.
    }
}

/// Space a row leaves left of its text: row padding, fold arrow, bullet and their gaps.
const TEXT_X: f32 = 8. + 14. + 4. + 12. + 4.;
/// Indentation per depth level (matches `render_block_row`).
const INDENT_PX: f32 = 24.;
/// Pointer further right than this beyond the text start means "as a child".
const CHILD_DX: f32 = 24.;
/// Fraction of the row height (from the top) that means "before".
const BEFORE_FRACTION: f32 = 0.4;

/// Where a dragged block lands relative to the row under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    /// As the previous sibling of the row.
    Before,
    /// As the next sibling of the row.
    After,
    /// As the first child of the row.
    Child,
}

impl DropZone {
    /// The core target for the row `id`.
    #[must_use]
    pub fn target(self, id: BlockId) -> Target {
        match self {
            Self::Before => Target::Before(id),
            Self::After => Target::After(id),
            Self::Child => Target::FirstChild(id),
        }
    }
}

/// The zone for a pointer at `local` (relative to the row's bounds): the upper part of the row
/// drops before it; in the lower part a pointer well right of the text, or a row with visible
/// children, drops as a child, otherwise after the row.
#[must_use]
pub fn zone_at(
    local: Point<Pixels>,
    height: Pixels,
    depth: usize,
    visible_children: bool,
) -> DropZone {
    let text_x = TEXT_X + depth as f32 * INDENT_PX;
    if height.as_f32() > 0. && local.y.as_f32() < height.as_f32() * BEFORE_FRACTION {
        DropZone::Before
    } else if visible_children || local.x.as_f32() > text_x + CHILD_DX {
        DropZone::Child
    } else {
        DropZone::After
    }
}

/// Left edge of the drop indicator of `zone` on a row of `depth`.
#[must_use]
pub fn indicator_left(zone: DropZone, depth: usize) -> Pixels {
    let base = TEXT_X + depth as f32 * INDENT_PX - 20.;
    px(match zone {
        DropZone::Before | DropZone::After => base,
        DropZone::Child => base + INDENT_PX,
    })
}

/// Whether `pos` is inside `bounds`.
#[must_use]
pub fn inside(bounds: &Bounds<Pixels>, pos: Point<Pixels>) -> bool {
    bounds.contains(&pos)
}

/// What a drag carries: the dragged blocks (top-level ones of a selection, or the one whose
/// bullet was grabbed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDrag {
    /// Blocks in reading order.
    pub ids: Vec<BlockId>,
    /// Text shown in the drag preview.
    pub label: SharedString,
}

/// The small card that follows the pointer.
#[derive(Debug)]
pub struct DragPreview {
    label: SharedString,
    count: usize,
}

impl DragPreview {
    /// A preview of `drag`.
    #[must_use]
    pub fn new(drag: &BlockDrag) -> Self {
        Self {
            label: drag.label.clone(),
            count: drag.ids.len(),
        }
    }
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let text = if self.count > 1 {
            format!("{} (+{})", self.label, self.count - 1)
        } else {
            self.label.to_string()
        };
        div()
            .max_w(px(320.))
            .px_2()
            .py_1()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .text_color(theme.foreground)
            .text_sm()
            .truncate()
            .shadow_md()
            .child(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{point, px};

    #[test]
    fn zones_follow_the_pointer() {
        let h = px(28.);
        // Upper part: before, wherever x is.
        assert_eq!(
            zone_at(point(px(300.), px(4.)), h, 0, false),
            DropZone::Before
        );
        assert_eq!(
            zone_at(point(px(10.), px(4.)), h, 0, true),
            DropZone::Before
        );
        // Lower part near the text start: after; further right: child.
        assert_eq!(
            zone_at(point(px(60.), px(22.)), h, 0, false),
            DropZone::After
        );
        assert_eq!(
            zone_at(point(px(120.), px(22.)), h, 0, false),
            DropZone::Child
        );
        // A row with visible children takes the block as its first child.
        assert_eq!(
            zone_at(point(px(60.), px(22.)), h, 0, true),
            DropZone::Child
        );
        // The child threshold moves with the depth.
        assert_eq!(
            zone_at(point(px(100.), px(22.)), h, 2, false),
            DropZone::After
        );
    }

    #[test]
    fn autoscroll_steps_grow_towards_the_edges() {
        use crate::ui::size;
        let view = Bounds::new(point(px(0.), px(100.)), size(px(800.), px(600.)));
        assert_eq!(scroll_step(px(400.), view), 0.);
        assert!(scroll_step(px(110.), view) < 0.);
        assert!(scroll_step(px(100.), view) < scroll_step(px(120.), view));
        assert!(scroll_step(px(690.), view) > 0.);
        assert!(scroll_step(px(700.), view) > scroll_step(px(660.), view));
        // Past the edge the step stays at its maximum.
        assert_eq!(scroll_step(px(900.), view), MAX_STEP);
    }

    #[test]
    fn zones_map_to_core_targets() {
        let id = BlockId::from_raw(7);
        assert_eq!(DropZone::Before.target(id), Target::Before(id));
        assert_eq!(DropZone::After.target(id), Target::After(id));
        assert_eq!(DropZone::Child.target(id), Target::FirstChild(id));
        assert!(indicator_left(DropZone::Child, 1) > indicator_left(DropZone::After, 1));
    }
}
