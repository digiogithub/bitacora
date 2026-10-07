//! What a rendered block row needs from the editor: the edit-mode content, selection state and
//! click callbacks. Built per row by [`super::OutlineEditor::row_edit`] and consumed by
//! `views::block_view::render_block_row`.

use std::rc::Rc;

use super::dnd::{BlockDrag, DropZone};
use crate::ui::theme::Theme;
use crate::ui::{AnyElement, App, Bounds, Pixels, Point, Window};

/// Callback with window access.
pub type Hook = Rc<dyn Fn(&mut Window, &mut App)>;

/// Click on rendered text: the block-text offset (`usize::MAX` = end of the block), whether
/// Shift was held.
pub type TextHook = Rc<dyn Fn(usize, bool, &mut Window, &mut App)>;

/// Files dropped on a row.
pub type DropHook = Rc<dyn Fn(&[std::path::PathBuf], &mut Window, &mut App)>;

/// The pointer moved while dragging blocks: payload, pointer, bounds of the row.
pub type DragMoveHook =
    Rc<dyn Fn(&BlockDrag, Point<Pixels>, Bounds<Pixels>, &mut Window, &mut App)>;

/// Dragged blocks were released on the row.
pub type DragDropHook = Rc<dyn Fn(&BlockDrag, &mut Window, &mut App)>;

/// Block drag and drop of one row (BIT-US-0106).
#[derive(Clone)]
pub struct RowDrag {
    /// What dragging this row's bullet carries.
    pub payload: BlockDrag,
    /// The drop indicator to draw on this row.
    pub zone: Option<DropZone>,
    /// Depth of the row (places the indicator).
    pub depth: usize,
    /// Pointer moved over the row during a drag.
    pub on_move: DragMoveHook,
    /// Blocks were dropped on the row.
    pub on_drop: DragDropHook,
}

/// Builds an element with the active theme.
pub type Build = Rc<dyn Fn(&Theme) -> AnyElement>;

/// Editor callbacks and state of one row.
#[derive(Clone)]
pub struct RowEdit {
    /// The block is selected (highlighted).
    pub selected: bool,
    /// The block is in edit mode: this builds its editor element.
    pub editing: Option<Build>,
    /// A line shown above the editor (the "changed on disk" choice), when there is one.
    pub conflict: Option<Build>,
    /// Click on rendered text or on the row's blank area.
    pub on_text: TextHook,
    /// The pointer moved over the row with the left button held (block drag selection).
    pub on_drag: Hook,
    /// Files were dropped on the row.
    pub on_drop: DropHook,
    /// "Delete asset" button, present when the block links to an asset file.
    pub on_delete_asset: Option<Hook>,
    /// Click on the task checkbox.
    pub on_checkbox: Hook,
    /// Click on the bullet (zoom into the block).
    pub on_bullet: Hook,
    /// Click on the fold arrow.
    pub on_toggle: Hook,
    /// Block drag and drop.
    pub drag: RowDrag,
    /// Click on a `SCHEDULED` / `DEADLINE` chip opens the date picker (BIT-US-0167).
    pub planning: Option<crate::views::planning::PlanningActions>,
}

impl std::fmt::Debug for RowDrag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowDrag")
            .field("payload", &self.payload)
            .field("zone", &self.zone)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for RowEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowEdit")
            .field("selected", &self.selected)
            .field("editing", &self.editing.is_some())
            .finish_non_exhaustive()
    }
}
