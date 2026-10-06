//! What a rendered block row needs from the editor: the edit-mode content, selection state and
//! click callbacks. Built per row by [`super::OutlineEditor::row_edit`] and consumed by
//! `views::block_view::render_block_row`.

use std::rc::Rc;

use crate::ui::{AnyElement, App, Window};

/// Callback with window access.
pub type Hook = Rc<dyn Fn(&mut Window, &mut App)>;

/// Click on rendered text: the block-text offset (`usize::MAX` = end of the block), whether
/// Shift was held.
pub type TextHook = Rc<dyn Fn(usize, bool, &mut Window, &mut App)>;

/// Editor callbacks and state of one row.
#[derive(Clone)]
pub struct RowEdit {
    /// The block is selected (highlighted).
    pub selected: bool,
    /// The block is in edit mode: this builds its editor element.
    pub editing: Option<Rc<dyn Fn() -> AnyElement>>,
    /// A line shown above the editor (the "changed on disk" choice), when there is one.
    pub conflict: Option<Rc<dyn Fn() -> AnyElement>>,
    /// Click on rendered text or on the row's blank area.
    pub on_text: TextHook,
    /// Click on the task checkbox.
    pub on_checkbox: Hook,
    /// Click on the bullet (zoom into the block).
    pub on_bullet: Hook,
    /// Click on the fold arrow.
    pub on_toggle: Hook,
}

impl std::fmt::Debug for RowEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowEdit")
            .field("selected", &self.selected)
            .field("editing", &self.editing.is_some())
            .finish_non_exhaustive()
    }
}
